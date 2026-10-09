//! Minimal Discord gateway client: receives live message/typing events on a
//! background thread and forwards them over a channel.

use std::net::TcpStream;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use rustls::pki_types::ServerName;
use serde_json::{json, Value};
use tungstenite::{client, Message as Ws};

use crate::api::Message;

const HOST: &str = "gateway.discord.gg";

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

/// Spawns the gateway thread. It stops (and the thread exits) once `stop` is set
/// or the receiver is dropped.
pub fn spawn(auth: String, tx: flume::Sender<Event>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        while !stop.load(Ordering::Relaxed) {
            let _ = run(&auth, &tx, &stop);
            if tx.send(Event::Connected(false)).is_err() {
                return;
            }
            // Back off before reconnecting.
            for _ in 0..50 {
                if stop.load(Ordering::Relaxed) {
                    return;
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
}

fn connect(
) -> Result<tungstenite::WebSocket<rustls::StreamOwned<rustls::ClientConnection, TcpStream>>, String>
{
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
    let name = ServerName::try_from(HOST).map_err(|e| e.to_string())?;
    let conn = rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    let tcp = TcpStream::connect((HOST, 443)).map_err(|e| e.to_string())?;
    let tls = rustls::StreamOwned::new(conn, tcp);
    let (ws, _) =
        client(format!("wss://{HOST}/?v=10&encoding=json"), tls).map_err(|e| e.to_string())?;
    Ok(ws)
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
            "properties": { "os": "Windows", "browser": "Chrome", "device": "" },
            "presence": { "status": "online", "since": 0, "activities": [], "afk": false },
            "compress": false,
        }})
    }
}

fn run(auth: &str, tx: &flume::Sender<Event>, stop: &AtomicBool) -> Result<(), String> {
    let mut ws = connect()?;
    ws.get_mut()
        .sock
        .set_read_timeout(Some(Duration::from_millis(500)))
        .map_err(|e| e.to_string())?;

    let mut interval = Duration::from_secs(40);
    let mut next_beat = Instant::now() + interval;
    let mut seq: Option<u64> = None;
    let mut identified = false;

    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next_beat {
            ws.send(Ws::text(json!({ "op": 1, "d": seq }).to_string()))
                .map_err(|e| e.to_string())?;
            next_beat = Instant::now() + interval;
        }
        let msg = match ws.read() {
            Ok(m) => m,
            Err(tungstenite::Error::Io(e))
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                ) =>
            {
                continue
            }
            Err(e) => return Err(e.to_string()),
        };
        let Ws::Text(text) = msg else {
            if matches!(msg, Ws::Close(_)) {
                return Ok(());
            }
            continue;
        };
        let Ok(v) = serde_json::from_str::<Value>(text.as_str()) else {
            continue;
        };
        if let Some(s) = v["s"].as_u64() {
            seq = Some(s);
        }
        match v["op"].as_u64() {
            Some(10) => {
                if let Some(ms) = v["d"]["heartbeat_interval"].as_u64() {
                    interval = Duration::from_millis(ms);
                    next_beat = Instant::now() + interval / 2;
                }
                if !identified {
                    ws.send(Ws::text(identify(auth).to_string()))
                        .map_err(|e| e.to_string())?;
                    identified = true;
                }
            }
            Some(1) => next_beat = Instant::now(),
            // Reconnect / invalid session: drop and let the caller reconnect.
            Some(7) | Some(9) => return Ok(()),
            Some(0) => {
                let ev = dispatch(v["t"].as_str().unwrap_or(""), &v["d"]);
                if let Some(ev) = ev {
                    if tx.send(ev).is_err() {
                        return Ok(());
                    }
                }
                if v["t"] == "READY" && tx.send(Event::Connected(true)).is_err() {
                    return Ok(());
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
