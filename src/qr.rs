//! QR-code login ("remote auth"): the user scans a code with the Discord mobile
//! app and the desktop receives a token. Protocol per the community-documented
//! remote-auth gateway: RSA-OAEP key exchange, nonce proof, ticket → token.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use rsa::pkcs8::EncodePublicKey;
use rsa::{Oaep, RsaPrivateKey};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tungstenite::Message as Ws;

use crate::{api, gateway};

#[derive(Debug)]
pub enum QrEvent {
    /// URL to encode in the QR code.
    Code(String),
    /// Someone scanned it; carries their username.
    Scanned(String),
    Token(String),
    Failed(String),
}

pub fn spawn(tx: flume::Sender<QrEvent>, stop: Arc<AtomicBool>) {
    std::thread::spawn(move || {
        let result = run(&tx, &stop);
        if let Err(e) = result {
            let _ = tx.send(QrEvent::Failed(e));
        }
    });
}

fn run(tx: &flume::Sender<QrEvent>, stop: &AtomicBool) -> Result<(), String> {
    let mut rng = rand::rngs::OsRng;
    let key = RsaPrivateKey::new(&mut rng, 2048).map_err(|e| e.to_string())?;
    let der = key
        .to_public_key()
        .to_public_key_der()
        .map_err(|e| e.to_string())?;
    let public_b64 = STANDARD.encode(der.as_bytes());
    let decrypt = |b64: &str| -> Result<Vec<u8>, String> {
        let raw = STANDARD.decode(b64).map_err(|e| e.to_string())?;
        key.decrypt(Oaep::new::<Sha256>(), &raw)
            .map_err(|e| e.to_string())
    };

    let mut ws = gateway::connect("wss://remote-auth-gateway.discord.gg/?v=2")?;
    gateway::set_timeout(&mut ws, Duration::from_millis(500));

    let mut interval = Duration::from_secs(40);
    let mut next_beat = Instant::now() + interval;
    let mut hello = false;

    while !stop.load(Ordering::Relaxed) {
        if hello && Instant::now() >= next_beat {
            ws.send(Ws::text(json!({ "op": "heartbeat" }).to_string()))
                .map_err(|e| e.to_string())?;
            next_beat = Instant::now() + interval;
        }
        let v: Value = match gateway::read_json(&mut ws) {
            Ok(Some(v)) => v,
            Ok(None) => continue,
            Err(()) => return Err("Code QR expiré. Actualisez pour en générer un nouveau.".into()),
        };
        match v["op"].as_str() {
            Some("hello") => {
                hello = true;
                if let Some(ms) = v["heartbeat_interval"].as_u64() {
                    interval = Duration::from_millis(ms);
                    next_beat = Instant::now() + interval;
                }
                ws.send(Ws::text(
                    json!({ "op": "init", "encoded_public_key": public_b64 }).to_string(),
                ))
                .map_err(|e| e.to_string())?;
            }
            Some("nonce_proof") => {
                let nonce = decrypt(v["encrypted_nonce"].as_str().unwrap_or_default())?;
                let proof = URL_SAFE_NO_PAD.encode(Sha256::digest(&nonce));
                ws.send(Ws::text(
                    json!({ "op": "nonce_proof", "nonce": proof }).to_string(),
                ))
                .map_err(|e| e.to_string())?;
            }
            Some("pending_remote_init") => {
                let fp = v["fingerprint"].as_str().unwrap_or_default();
                let _ = tx.send(QrEvent::Code(format!("https://discord.com/ra/{fp}")));
            }
            Some("pending_ticket") => {
                let payload = decrypt(v["encrypted_user_payload"].as_str().unwrap_or_default())?;
                // "id:discriminator:avatar:username"
                let text = String::from_utf8_lossy(&payload).to_string();
                let name = text.splitn(4, ':').nth(3).unwrap_or("").to_string();
                let _ = tx.send(QrEvent::Scanned(name));
            }
            Some("pending_login") => {
                let ticket = v["ticket"].as_str().unwrap_or_default().to_string();
                let enc = api::remote_auth_login(&ticket)?;
                let token = String::from_utf8(decrypt(&enc)?).map_err(|e| e.to_string())?;
                let _ = tx.send(QrEvent::Token(token));
                return Ok(());
            }
            Some("cancel") => return Err("Connexion annulée sur le téléphone.".into()),
            _ => {}
        }
    }
    Ok(())
}
