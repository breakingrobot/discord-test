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

use crate::api::{self, Message};

const DEFAULT_URL: &str = "wss://gateway.discord.gg/?v=9&encoding=json";

pub type Socket = tungstenite::WebSocket<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>;

#[derive(Debug)]
pub enum Event {
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
pub fn spawn(auth: String, tx: flume::Sender<Event>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let mut session = Session::default();
        let mut failures = 0u32;
        while !stop.load(Ordering::Relaxed) {
            let started = Instant::now();
            let _ = run(&auth, &tx, &stop, &mut session);
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
        "TYPING_START" => Some(Event::Typing {
            channel_id: s("channel_id"),
            user_id: s("user_id"),
        }),
        _ => None,
    }
}
