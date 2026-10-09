//! Discord gateway client: receives live events on a background thread and
//! forwards them over a channel. Resumes dropped sessions and reconnects with
//! exponential backoff (rapid reconnects are what trips Discord's spam filter).

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use serde_json::{json, Value};
use tungstenite::client::IntoClientRequest;
use tungstenite::http::HeaderValue;
use tungstenite::{client, Message as Ws};

use crate::api::{self, Message, Role, User};

const DEFAULT_URL: &str = "wss://gateway.discord.gg/?v=9&encoding=json";

pub type Socket = tungstenite::WebSocket<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>;

/// Commands from the UI to the gateway thread.
#[derive(Debug)]
pub enum Command {
    /// Subscribe to a guild's member list (user accounts only).
    RequestMembers {
        guild_id: String,
        channel_id: String,
    },
}

#[derive(Debug, Clone)]
pub enum MemberRow {
    Group {
        id: String,
        count: u64,
    },
    Member {
        user: User,
        status: String,
        roles: Vec<String>,
    },
}

#[derive(Debug)]
pub enum Event {
    Members {
        guild_id: String,
        rows: Vec<MemberRow>,
    },
    /// The member list changed in a way we don't patch incrementally.
    MembersStale {
        guild_id: String,
    },
    /// Roles per guild, taken from READY (avoids a REST call per guild).
    Roles(Vec<(String, Vec<(String, Role)>)>),
    /// Our session id (needed to run slash commands).
    Session(String),
    /// Initial presences of friends: (user id, status).
    Presences(Vec<(String, String)>),
    Presence {
        user_id: String,
        status: String,
    },
    /// A thread was created / updated / deleted in this guild.
    ThreadsChanged {
        guild_id: String,
    },
    MessageCreate(Message),
    /// A message changed (edit, reaction, embed resolved) in this channel.
    ChannelChanged(String),
    MessageDelete {
        channel_id: String,
        id: String,
    },
    Typing {
        channel_id: String,
        user_id: String,
    },
    Connected(bool),
}

/// Opens a TLS websocket to `url`, sending the web client's `Origin`.
pub fn connect(url: &str) -> Result<Socket, String> {
    let mut req = url.into_client_request().map_err(|e| e.to_string())?;
    req.headers_mut()
        .insert("Origin", HeaderValue::from_static("https://discord.com"));
    let host = req.uri().host().ok_or("URL sans hôte")?.to_string();

    let roots = rustls::RootCertStore {
        roots: webpki_roots::TLS_SERVER_ROOTS.to_vec(),
    };
    let config = rustls::ClientConfig::builder_with_provider(Arc::new(
        rustls::crypto::ring::default_provider(),
    ))
    .with_safe_default_protocol_versions()
    .map_err(|e| e.to_string())?
    .with_root_certificates(roots)
    .with_no_client_auth();
    let name = ServerName::try_from(host.clone()).map_err(|e| e.to_string())?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    let tcp = TcpStream::connect((host.as_str(), 443)).map_err(|e| e.to_string())?;
    let tls = rustls::StreamOwned::new(conn, tcp);
    let (ws, _) = client(req, tls).map_err(|e| e.to_string())?;
    Ok(ws)
}

/// Reads one text frame, returning `Ok(None)` on read timeout / non-text frames.
/// `Err(())` means the connection is closed or broken.
pub fn read_json(ws: &mut Socket) -> Result<Option<Value>, ()> {
    match ws.read() {
        Ok(Ws::Text(t)) => Ok(serde_json::from_str(t.as_str()).ok()),
        Ok(Ws::Close(_)) => Err(()),
        Ok(_) => Ok(None),
        Err(tungstenite::Error::Io(e))
            if matches!(
                e.kind(),
                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
            ) =>
        {
            Ok(None)
        }
        Err(_) => Err(()),
    }
}

pub fn set_timeout(ws: &mut Socket, d: Duration) {
    let _ = ws.get_mut().sock.set_read_timeout(Some(d));
}

#[derive(Default)]
struct Session {
    id: Option<String>,
    resume_url: Option<String>,
    seq: Option<u64>,
}

/// Spawns the gateway thread. It stops once `stop` is set or the receiver is dropped.
pub fn spawn(
    auth: String,
    tx: flume::Sender<Event>,
    stop: Arc<AtomicBool>,
) -> flume::Sender<Command> {
    let (cmd_tx, cmd_rx) = flume::unbounded();
    std::thread::spawn(move || {
        let mut session = Session::default();
        let mut failures = 0u32;
        while !stop.load(Ordering::Relaxed) {
            let started = Instant::now();
            let _ = run(&auth, &tx, &cmd_rx, &stop, &mut session);
            if tx.send(Event::Connected(false)).is_err() {
                return;
            }
            // A connection that lived a while resets the backoff.
            if started.elapsed() > Duration::from_secs(60) {
                failures = 0;
            }
            failures = (failures + 1).min(6);
            let wait = Duration::from_secs(2u64.pow(failures)); // 2s … 64s
            let until = Instant::now() + wait;
            while Instant::now() < until {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    });
    cmd_tx
}

fn identify(auth: &str) -> Value {
    if let Some(bot) = auth.strip_prefix("Bot ") {
        // guilds | guild messages | guild message typing | DMs | message content
        json!({ "op": 2, "d": {
            "token": bot,
            "intents": 1 | (1 << 9) | (1 << 11) | (1 << 12) | (1 << 15),
            "properties": { "os": "linux", "browser": "discord-test", "device": "discord-test" },
        }})
    } else {
        json!({ "op": 2, "d": {
            "token": auth,
            "capabilities": 16381,
            "properties": api::super_properties(),
            "presence": { "status": "online", "since": 0, "activities": [], "afk": false },
            "compress": false,
            "client_state": { "guild_versions": {} },
        }})
    }
}

fn run(
    auth: &str,
    tx: &flume::Sender<Event>,
    cmds: &flume::Receiver<Command>,
    stop: &AtomicBool,
    session: &mut Session,
) -> Result<(), String> {
    let url = match (&session.id, &session.resume_url) {
        (Some(_), Some(u)) => format!("{u}/?v=9&encoding=json"),
        _ => DEFAULT_URL.to_string(),
    };
    let mut ws = connect(&url)?;
    set_timeout(&mut ws, Duration::from_millis(500));

    let mut interval = Duration::from_secs(40);
    let mut next_beat = Instant::now() + interval;
    let mut hello = false;

    while !stop.load(Ordering::Relaxed) {
        if hello && Instant::now() >= next_beat {
            ws.send(Ws::text(json!({ "op": 1, "d": session.seq }).to_string()))
                .map_err(|e| e.to_string())?;
            next_beat = Instant::now() + interval;
        }
        while let Ok(cmd) = cmds.try_recv() {
            let Command::RequestMembers {
                guild_id,
                channel_id,
            } = cmd;
            if session.id.is_some() && !auth.starts_with("Bot ") {
                let msg = json!({ "op": 14, "d": {
                    "guild_id": guild_id,
                    "typing": true, "activities": true, "threads": true,
                    "channels": { channel_id: [[0, 99]] },
                }});
                ws.send(Ws::text(msg.to_string()))
                    .map_err(|e| e.to_string())?;
            }
        }
        let v = match read_json(&mut ws) {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(()) => return Ok(()),
        };
        if let Some(s) = v["s"].as_u64() {
            session.seq = Some(s);
        }
        match v["op"].as_u64() {
            Some(10) => {
                hello = true;
                if let Some(ms) = v["d"]["heartbeat_interval"].as_u64() {
                    interval = Duration::from_millis(ms);
                    // First beat after a random fraction of the interval, like the web client.
                    next_beat = Instant::now() + interval.mul_f64(0.5);
                }
                let first = match (&session.id, session.seq) {
                    (Some(id), Some(seq)) => json!({ "op": 6, "d": {
                        "token": auth.strip_prefix("Bot ").unwrap_or(auth),
                        "session_id": id, "seq": seq,
                    }}),
                    _ => identify(auth),
                };
                ws.send(Ws::text(first.to_string()))
                    .map_err(|e| e.to_string())?;
            }
            Some(1) => next_beat = Instant::now(),
            Some(7) => return Ok(()), // server asks us to reconnect (resume)
            Some(9) => {
                // Invalid session: resumable only when d == true.
                if v["d"] != true {
                    *session = Session::default();
                }
                return Ok(());
            }
            Some(0) => {
                let kind = v["t"].as_str().unwrap_or("");
                if kind == "READY" {
                    session.id = v["d"]["session_id"].as_str().map(str::to_string);
                    session.resume_url = v["d"]["resume_gateway_url"].as_str().map(str::to_string);
                    if tx.send(Event::Connected(true)).is_err() {
                        return Ok(());
                    }
                    if let Some(id) = &session.id {
                        let _ = tx.send(Event::Session(id.clone()));
                    }
                    let _ = tx.send(Event::Presences(ready_presences(&v["d"])));
                    let _ = tx.send(Event::Roles(ready_roles(&v["d"])));
                } else if kind == "RESUMED" && tx.send(Event::Connected(true)).is_err() {
                    return Ok(());
                }
                if let Some(ev) = dispatch(kind, &v["d"]) {
                    if tx.send(ev).is_err() {
                        return Ok(());
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Guild roles carried by READY's `guilds` (user accounts).
fn ready_roles(d: &Value) -> Vec<(String, Vec<(String, Role)>)> {
    d["guilds"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|g| {
            let id = g["id"].as_str()?.to_string();
            let roles = g["roles"]
                .as_array()?
                .iter()
                .filter_map(|r| {
                    Some((
                        r["id"].as_str()?.to_string(),
                        Role {
                            name: r["name"].as_str()?.to_string(),
                            color: r["color"].as_u64().unwrap_or(0) as u32,
                            position: r["position"].as_i64().unwrap_or(0),
                        },
                    ))
                })
                .collect();
            Some((id, roles))
        })
        .collect()
}

/// Friend presences from READY (shape differs between gateway versions).
fn ready_presences(d: &Value) -> Vec<(String, String)> {
    let lists = [&d["merged_presences"]["friends"], &d["presences"]];
    let mut out = Vec::new();
    for list in lists {
        for p in list.as_array().into_iter().flatten() {
            let id = p["user_id"].as_str().or_else(|| p["user"]["id"].as_str());
            if let (Some(id), Some(st)) = (id, p["status"].as_str()) {
                out.push((id.to_string(), st.to_string()));
            }
        }
    }
    out
}

fn dispatch(kind: &str, d: &Value) -> Option<Event> {
    let s = |k: &str| d[k].as_str().unwrap_or_default().to_string();
    match kind {
        "MESSAGE_CREATE" => serde_json::from_value::<Message>(d.clone())
            .ok()
            .map(Event::MessageCreate),
        "MESSAGE_UPDATE"
        | "MESSAGE_REACTION_ADD"
        | "MESSAGE_REACTION_REMOVE"
        | "MESSAGE_REACTION_REMOVE_ALL"
        | "MESSAGE_REACTION_REMOVE_EMOJI" => Some(Event::ChannelChanged(s("channel_id"))),
        "MESSAGE_DELETE" => Some(Event::MessageDelete {
            channel_id: s("channel_id"),
            id: s("id"),
        }),
        "GUILD_MEMBER_LIST_UPDATE" => {
            let guild_id = s("guild_id");
            let mut rows = Vec::new();
            let mut synced = false;
            for op in d["ops"].as_array().into_iter().flatten() {
                if op["op"] == "SYNC" {
                    synced = true;
                    for item in op["items"].as_array().into_iter().flatten() {
                        if let Some(gr) = item.get("group") {
                            rows.push(MemberRow::Group {
                                id: gr["id"].as_str().unwrap_or_default().to_string(),
                                count: gr["count"].as_u64().unwrap_or(0),
                            });
                        } else if let Some(m) = item.get("member") {
                            if let Ok(mut user) = serde_json::from_value::<User>(m["user"].clone())
                            {
                                if let Some(nick) = m["nick"].as_str() {
                                    user.global_name = Some(nick.to_string());
                                }
                                let roles = m["roles"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(|r| r.as_str().map(str::to_string))
                                    .collect();
                                rows.push(MemberRow::Member {
                                    roles,
                                    user,
                                    status: m["presence"]["status"]
                                        .as_str()
                                        .unwrap_or("offline")
                                        .to_string(),
                                });
                            }
                        }
                    }
                }
            }
            Some(if synced {
                Event::Members { guild_id, rows }
            } else {
                Event::MembersStale { guild_id }
            })
        }
        "PRESENCE_UPDATE" => Some(Event::Presence {
            user_id: d["user"]["id"].as_str().unwrap_or_default().to_string(),
            status: s("status"),
        }),
        "THREAD_CREATE" | "THREAD_UPDATE" | "THREAD_DELETE" => Some(Event::ThreadsChanged {
            guild_id: s("guild_id"),
        }),
        "TYPING_START" => Some(Event::Typing {
            channel_id: s("channel_id"),
            user_id: s("user_id"),
        }),
        _ => None,
    }
}
