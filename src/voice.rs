//! Discord voice: voice gateway v8 with DAVE end-to-end encryption, RTP over
//! UDP with AEAD transport encryption, Opus, microphone / speaker I/O through
//! cpal and RNNoise noise suppression (nnnoiseless).
//!
//! Threads: one for the voice websocket (signalling + DAVE MLS), one sending
//! audio every 20 ms, one receiving and decoding, plus the cpal audio callbacks.

use std::collections::{HashMap, HashSet, VecDeque};
use std::net::UdpSocket;
use std::num::NonZeroU16;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use aes_gcm::aead::{Aead, KeyInit, Payload};
use aes_gcm::Aes256Gcm;
use audiopus::coder::{Decoder, Encoder};
use audiopus::{Application, Channels, MutSignals, SampleRate};
use chacha20poly1305::XChaCha20Poly1305;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use davey::{DaveSession, MediaType, ProposalsOperationType};
use serde_json::{json, Value};
use tungstenite::Message as Ws;

use crate::gateway;

const SAMPLE_RATE: usize = 48_000;
/// 20 ms of audio at 48 kHz.
const FRAME: usize = 960;
const OPUS_SILENCE: [u8; 3] = [0xF8, 0xFF, 0xFE];
const PAYLOAD_TYPE: u8 = 120;

/// Everything needed to open a voice connection (from the main gateway).
#[derive(Clone, Debug)]
pub struct ConnectInfo {
    pub endpoint: String,
    pub token: String,
    pub session_id: String,
    /// Guild id, or the DM channel id for calls.
    pub server_id: String,
    pub channel_id: String,
    pub user_id: String,
}

#[derive(Debug)]
pub enum VoiceEvent {
    /// Human-readable connection state.
    State(String),
    Connected,
    Speaking {
        user_id: String,
        speaking: bool,
    },
    /// DAVE voice privacy code once the E2EE group is established.
    Privacy(Option<String>),
    Closed(String),
}

#[derive(Clone)]
pub struct VoiceSettings {
    pub muted: Arc<AtomicBool>,
    pub deafened: Arc<AtomicBool>,
    pub noise_suppression: Arc<AtomicBool>,
    pub input_device: Option<String>,
    pub output_device: Option<String>,
}

impl Default for VoiceSettings {
    fn default() -> Self {
        Self {
            muted: Arc::new(AtomicBool::new(false)),
            deafened: Arc::new(AtomicBool::new(false)),
            noise_suppression: Arc::new(AtomicBool::new(true)),
            input_device: None,
            output_device: None,
        }
    }
}

/// Handle kept by the UI; dropping or calling `stop` tears the connection down.
pub struct VoiceHandle {
    stop: Arc<AtomicBool>,
}

impl VoiceHandle {
    pub fn stop(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

impl Drop for VoiceHandle {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Input / output device names for the settings screen.
pub fn devices() -> (Vec<String>, Vec<String>) {
    let host = cpal::default_host();
    let names = |it: Option<Vec<cpal::Device>>| {
        it.unwrap_or_default()
            .into_iter()
            .filter_map(|d| d.name().ok())
            .collect::<Vec<_>>()
    };
    (
        names(host.input_devices().ok().map(|i| i.collect())),
        names(host.output_devices().ok().map(|i| i.collect())),
    )
}

pub fn connect(
    info: ConnectInfo,
    settings: VoiceSettings,
    events: flume::Sender<VoiceEvent>,
) -> VoiceHandle {
    let stop = Arc::new(AtomicBool::new(false));
    let s = stop.clone();
    std::thread::spawn(move || {
        let reason = match run(&info, &settings, &events, &s) {
            Ok(()) => "Déconnecté".to_string(),
            Err(e) => e,
        };
        s.store(true, Ordering::Relaxed);
        let _ = events.send(VoiceEvent::Closed(reason));
    });
    VoiceHandle { stop }
}

// ---- shared state --------------------------------------------------------------

/// Transport encryption for one session.
enum Cipher {
    Aes(Box<Aes256Gcm>),
    XChaCha(Box<XChaCha20Poly1305>),
}

impl Cipher {
    fn new(mode: &str, key: &[u8]) -> Option<Cipher> {
        match mode {
            "aead_aes256_gcm_rtpsize" => Aes256Gcm::new_from_slice(key)
                .ok()
                .map(|c| Cipher::Aes(Box::new(c))),
            "aead_xchacha20_poly1305_rtpsize" => XChaCha20Poly1305::new_from_slice(key)
                .ok()
                .map(|c| Cipher::XChaCha(Box::new(c))),
            _ => None,
        }
    }

    fn seal(&self, counter: u32, aad: &[u8], plain: &[u8]) -> Option<Vec<u8>> {
        let payload = Payload { msg: plain, aad };
        match self {
            Cipher::Aes(c) => {
                let mut n = [0u8; 12];
                n[..4].copy_from_slice(&counter.to_be_bytes());
                c.encrypt((&n).into(), payload).ok()
            }
            Cipher::XChaCha(c) => {
                let mut n = [0u8; 24];
                n[..4].copy_from_slice(&counter.to_be_bytes());
                c.encrypt((&n).into(), payload).ok()
            }
        }
    }

    fn open(&self, nonce4: &[u8], aad: &[u8], sealed: &[u8]) -> Option<Vec<u8>> {
        let payload = Payload { msg: sealed, aad };
        match self {
            Cipher::Aes(c) => {
                let mut n = [0u8; 12];
                n[..4].copy_from_slice(nonce4);
                c.decrypt((&n).into(), payload).ok()
            }
            Cipher::XChaCha(c) => {
                let mut n = [0u8; 24];
                n[..4].copy_from_slice(nonce4);
                c.decrypt((&n).into(), payload).ok()
            }
        }
    }
}

struct Shared {
    stop: Arc<AtomicBool>,
    cipher: Mutex<Option<Cipher>>,
    ssrc: AtomicU32,
    /// SSRC -> user id, from Speaking events.
    users: Mutex<HashMap<u32, u64>>,
    dave: Mutex<Option<DaveSession>>,
    /// True once the E2EE transition is executed (send with DAVE).
    dave_active: AtomicBool,
    /// Per-SSRC decoded PCM (stereo, interleaved f32) waiting to be played.
    playback: Mutex<HashMap<u32, VecDeque<f32>>>,
    /// Mono f32 microphone samples at 48 kHz.
    capture: Mutex<VecDeque<f32>>,
    /// Set by the sender when our speaking state flips; the ws thread relays it.
    speaking_changed: Mutex<Option<bool>>,
}

// ---- websocket / signalling ---------------------------------------------------------

fn send_json(ws: &mut gateway::Socket, v: Value) -> Result<(), String> {
    ws.send(Ws::text(v.to_string())).map_err(|e| e.to_string())
}

fn send_binary(ws: &mut gateway::Socket, op: u8, parts: &[&[u8]]) -> Result<(), String> {
    let mut buf = vec![op];
    for p in parts {
        buf.extend_from_slice(p);
    }
    ws.send(Ws::binary(buf)).map_err(|e| e.to_string())
}

fn run(
    info: &ConnectInfo,
    settings: &VoiceSettings,
    events: &flume::Sender<VoiceEvent>,
    stop: &Arc<AtomicBool>,
) -> Result<(), String> {
    let _ = events.send(VoiceEvent::State("Connexion au serveur vocal…".into()));
    let host = info.endpoint.trim_start_matches("wss://");
    let mut ws = gateway::connect(&format!("wss://{host}/?v=8"))?;
    gateway::set_timeout(&mut ws, Duration::from_millis(50));

    let user_id: u64 = info.user_id.parse().unwrap_or(0);
    let channel_id: u64 = info.channel_id.parse().unwrap_or(0);
    send_json(
        &mut ws,
        json!({ "op": 0, "d": {
            "server_id": info.server_id,
            "channel_id": info.channel_id,
            "user_id": info.user_id,
            "session_id": info.session_id,
            "token": info.token,
            "video": false,
            "max_dave_protocol_version": davey::DAVE_PROTOCOL_VERSION,
        }}),
    )?;

    let shared = Arc::new(Shared {
        stop: stop.clone(),
        cipher: Mutex::new(None),
        ssrc: AtomicU32::new(0),
        users: Mutex::new(HashMap::new()),
        dave: Mutex::new(None),
        dave_active: AtomicBool::new(false),
        playback: Mutex::new(HashMap::new()),
        capture: Mutex::new(VecDeque::new()),
        speaking_changed: Mutex::new(None),
    });

    let mut interval = Duration::from_secs(13);
    let mut next_beat = Instant::now() + interval;
    let mut seq_ack: i64 = -1;
    let mut udp: Option<Arc<UdpSocket>> = None;
    let mut audio: Option<std::thread::JoinHandle<()>> = None;
    let mut recognized: HashSet<u64> = HashSet::from([user_id]);
    let mut external_sender: Option<Vec<u8>> = None;
    let mut dave_version: u16 = 0;
    // Pending downgrade (transition id -> protocol version).
    let mut pending_version: HashMap<u16, u16> = HashMap::new();

    let reinit = |shared: &Shared, version: u16, ext: &Option<Vec<u8>>| -> Option<Vec<u8>> {
        let v = NonZeroU16::new(version)?;
        let mut guard = shared.dave.lock().ok()?;
        let session = match guard.as_mut() {
            Some(s) => {
                s.reinit(v, user_id, channel_id, None).ok()?;
                s
            }
            None => {
                *guard = Some(DaveSession::new(v, user_id, channel_id, None).ok()?);
                guard.as_mut()?
            }
        };
        if let Some(ext) = ext {
            let _ = session.set_external_sender(ext);
        }
        session.create_key_package().ok()
    };

    while !stop.load(Ordering::Relaxed) {
        if Instant::now() >= next_beat {
            let t = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis() as u64)
                .unwrap_or(0);
            send_json(
                &mut ws,
                json!({ "op": 3, "d": { "t": t, "seq_ack": seq_ack } }),
            )?;
            next_beat = Instant::now() + interval;
        }
        if let Some(sp) = shared
            .speaking_changed
            .lock()
            .ok()
            .and_then(|mut s| s.take())
        {
            send_json(
                &mut ws,
                json!({ "op": 5, "d": {
                    "speaking": if sp { 1 } else { 0 },
                    "delay": 0,
                    "ssrc": shared.ssrc.load(Ordering::Relaxed),
                }}),
            )?;
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
            Err(e) => return Err(format!("Connexion vocale perdue : {e}")),
        };
        match msg {
            Ws::Close(frame) => {
                let code = frame.as_ref().map(|f| u16::from(f.code)).unwrap_or(0);
                return Err(match code {
                    4014 | 4022 => "Déconnecté du salon vocal".into(),
                    4017 => "Ce salon exige le chiffrement de bout en bout (DAVE)".into(),
                    0 => "Connexion vocale fermée".into(),
                    c => format!("Connexion vocale fermée ({c})"),
                });
            }
            Ws::Binary(bytes) => {
                if bytes.len() < 3 {
                    continue;
                }
                seq_ack = u16::from_be_bytes([bytes[0], bytes[1]]) as i64;
                let op = bytes[2];
                let payload = &bytes[3..];
                match op {
                    // External sender package.
                    25 => {
                        external_sender = Some(payload.to_vec());
                        if dave_version > 0 {
                            if let Some(kp) = reinit(&shared, dave_version, &external_sender) {
                                send_binary(&mut ws, 26, &[&kp])?;
                            }
                        }
                    }
                    // Proposals: [operation type][proposals].
                    27 if !payload.is_empty() => {
                        let optype = if payload[0] == 1 {
                            ProposalsOperationType::REVOKE
                        } else {
                            ProposalsOperationType::APPEND
                        };
                        let ids: Vec<u64> = recognized.iter().copied().collect();
                        let cw = shared.dave.lock().ok().and_then(|mut g| {
                            g.as_mut()?
                                .process_proposals(optype, &payload[1..], Some(&ids))
                                .ok()?
                        });
                        if let Some(cw) = cw {
                            match &cw.welcome {
                                Some(w) => send_binary(&mut ws, 28, &[&cw.commit, w])?,
                                None => send_binary(&mut ws, 28, &[&cw.commit])?,
                            }
                        }
                    }
                    // Announce commit / welcome: [transition id u16][message].
                    29 | 30 if payload.len() > 2 => {
                        let tid = u16::from_be_bytes([payload[0], payload[1]]);
                        let ok = shared.dave.lock().ok().is_some_and(|mut g| {
                            g.as_mut().is_some_and(|s| {
                                if op == 29 {
                                    s.process_commit(&payload[2..]).is_ok()
                                } else {
                                    s.process_welcome(&payload[2..]).is_ok()
                                }
                            })
                        });
                        if ok {
                            if tid != 0 {
                                pending_version.insert(tid, dave_version);
                                send_json(
                                    &mut ws,
                                    json!({ "op": 23, "d": { "transition_id": tid } }),
                                )?;
                            } else {
                                shared.dave_active.store(true, Ordering::Relaxed);
                            }
                            let code =
                                shared.dave.lock().ok().and_then(|g| {
                                    g.as_ref()?.voice_privacy_code().map(str::to_string)
                                });
                            let _ = events.send(VoiceEvent::Privacy(code));
                        } else {
                            send_json(&mut ws, json!({ "op": 31, "d": { "transition_id": tid } }))?;
                            if let Some(kp) = reinit(&shared, dave_version, &external_sender) {
                                send_binary(&mut ws, 26, &[&kp])?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            Ws::Text(text) => {
                let Ok(v) = serde_json::from_str::<Value>(text.as_str()) else {
                    continue;
                };
                if let Some(s) = v["seq"].as_i64() {
                    seq_ack = s;
                }
                let d = &v["d"];
                match v["op"].as_u64() {
                    // Hello
                    Some(8) => {
                        if let Some(ms) = d["heartbeat_interval"].as_f64() {
                            interval = Duration::from_millis(ms as u64);
                            next_beat = Instant::now() + interval;
                        }
                    }
                    // Ready: open UDP, discover our address, select protocol.
                    Some(2) => {
                        let ssrc = d["ssrc"].as_u64().unwrap_or(0) as u32;
                        shared.ssrc.store(ssrc, Ordering::Relaxed);
                        let ip = d["ip"].as_str().unwrap_or_default();
                        let port = d["port"].as_u64().unwrap_or(0) as u16;
                        let modes: Vec<&str> = d["modes"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|m| m.as_str())
                            .collect();
                        let mode = if modes.contains(&"aead_aes256_gcm_rtpsize") {
                            "aead_aes256_gcm_rtpsize"
                        } else {
                            "aead_xchacha20_poly1305_rtpsize"
                        };
                        let sock = UdpSocket::bind("0.0.0.0:0").map_err(|e| e.to_string())?;
                        sock.connect((ip, port)).map_err(|e| e.to_string())?;
                        let (addr, ext_port) = ip_discovery(&sock, ssrc)?;
                        udp = Some(Arc::new(sock));
                        send_json(
                            &mut ws,
                            json!({ "op": 1, "d": {
                                "protocol": "udp",
                                "data": { "address": addr, "port": ext_port, "mode": mode },
                                "codecs": [{ "name": "opus", "type": "audio", "priority": 1000, "payload_type": PAYLOAD_TYPE }],
                            }}),
                        )?;
                    }
                    // Session description: keys + DAVE version; start audio.
                    Some(4) => {
                        let key: Vec<u8> = d["secret_key"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .filter_map(|b| b.as_u64().map(|b| b as u8))
                            .collect();
                        let mode = d["mode"].as_str().unwrap_or_default();
                        let cipher =
                            Cipher::new(mode, &key).ok_or("Mode de chiffrement inconnu")?;
                        if let Ok(mut c) = shared.cipher.lock() {
                            *c = Some(cipher);
                        }
                        dave_version = d["dave_protocol_version"].as_u64().unwrap_or(0) as u16;
                        if dave_version > 0 {
                            if let Some(kp) = reinit(&shared, dave_version, &external_sender) {
                                send_binary(&mut ws, 26, &[&kp])?;
                            }
                        }
                        if audio.is_none() {
                            if let Some(sock) = udp.clone() {
                                audio = Some(start_audio(
                                    shared.clone(),
                                    sock,
                                    settings.clone(),
                                    events.clone(),
                                ));
                            }
                        }
                        let _ = events.send(VoiceEvent::Connected);
                    }
                    // Speaking: SSRC -> user id mapping.
                    Some(5) => {
                        if let (Some(ssrc), Some(uid)) = (
                            d["ssrc"].as_u64(),
                            d["user_id"].as_str().and_then(|u| u.parse().ok()),
                        ) {
                            if let Ok(mut m) = shared.users.lock() {
                                m.insert(ssrc as u32, uid);
                            }
                        }
                    }
                    // Clients connect / disconnect.
                    Some(11) => {
                        for u in d["user_ids"].as_array().into_iter().flatten() {
                            if let Some(id) = u.as_str().and_then(|s| s.parse().ok()) {
                                recognized.insert(id);
                            }
                        }
                    }
                    Some(13) => {
                        if let Some(id) = d["user_id"].as_str().and_then(|s| s.parse::<u64>().ok())
                        {
                            recognized.remove(&id);
                        }
                    }
                    // DAVE prepare transition (downgrade).
                    Some(21) => {
                        let tid = d["transition_id"].as_u64().unwrap_or(0) as u16;
                        let ver = d["protocol_version"].as_u64().unwrap_or(0) as u16;
                        if tid == 0 {
                            shared.dave_active.store(ver > 0, Ordering::Relaxed);
                        } else {
                            pending_version.insert(tid, ver);
                            send_json(&mut ws, json!({ "op": 23, "d": { "transition_id": tid } }))?;
                        }
                    }
                    // Execute transition.
                    Some(22) => {
                        let tid = d["transition_id"].as_u64().unwrap_or(0) as u16;
                        if let Some(ver) = pending_version.remove(&tid) {
                            dave_version = ver;
                            shared.dave_active.store(ver > 0, Ordering::Relaxed);
                        }
                    }
                    // Prepare epoch: epoch 1 means a brand new group.
                    Some(24) => {
                        let ver = d["protocol_version"].as_u64().unwrap_or(0) as u16;
                        if d["epoch"].as_u64() == Some(1) {
                            dave_version = ver;
                            if let Some(kp) = reinit(&shared, ver, &external_sender) {
                                send_binary(&mut ws, 26, &[&kp])?;
                            }
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Sends the 74-byte discovery request and parses our public address.
fn ip_discovery(sock: &UdpSocket, ssrc: u32) -> Result<(String, u16), String> {
    let mut req = [0u8; 74];
    req[0..2].copy_from_slice(&1u16.to_be_bytes());
    req[2..4].copy_from_slice(&70u16.to_be_bytes());
    req[4..8].copy_from_slice(&ssrc.to_be_bytes());
    sock.set_read_timeout(Some(Duration::from_secs(3)))
        .map_err(|e| e.to_string())?;
    for _ in 0..3 {
        sock.send(&req).map_err(|e| e.to_string())?;
        let mut buf = [0u8; 74];
        if let Ok(n) = sock.recv(&mut buf) {
            if n >= 74 && buf[1] == 2 {
                let ip_end = buf[8..72].iter().position(|b| *b == 0).unwrap_or(64);
                let ip = String::from_utf8_lossy(&buf[8..8 + ip_end]).to_string();
                let port = u16::from_be_bytes([buf[72], buf[73]]);
                return Ok((ip, port));
            }
        }
    }
    Err("Découverte d'adresse UDP impossible".into())
}

// ---- audio ---------------------------------------------------------------------------

fn find_device(input: bool, name: &Option<String>) -> Option<cpal::Device> {
    let host = cpal::default_host();
    if let Some(name) = name {
        let found = if input {
            host.input_devices()
                .ok()?
                .find(|d| d.name().ok().as_deref() == Some(name))
        } else {
            host.output_devices()
                .ok()?
                .find(|d| d.name().ok().as_deref() == Some(name))
        };
        if found.is_some() {
            return found;
        }
    }
    if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    }
}

/// Starts capture / playback streams and the send / receive loops.
fn start_audio(
    shared: Arc<Shared>,
    sock: Arc<UdpSocket>,
    settings: VoiceSettings,
    events: flume::Sender<VoiceEvent>,
) -> std::thread::JoinHandle<()> {
    // Receive loop.
    {
        let (shared, sock, settings, events) = (
            shared.clone(),
            sock.clone(),
            settings.clone(),
            events.clone(),
        );
        std::thread::spawn(move || receive_loop(shared, sock, settings, events));
    }
    // Send loop.
    {
        let (shared, sock, settings, events) = (
            shared.clone(),
            sock.clone(),
            settings.clone(),
            events.clone(),
        );
        std::thread::spawn(move || send_loop(shared, sock, settings, events));
    }
    // cpal streams live on their own thread (they are not Send on every platform).
    std::thread::spawn(move || {
        let input = find_device(true, &settings.input_device).and_then(|dev| {
            let cfg = dev.default_input_config().ok()?;
            let channels = cfg.channels() as usize;
            let rate = cfg.sample_rate().0 as f64;
            let cap = shared.clone();
            let mut resampler = Resampler::new(rate, SAMPLE_RATE as f64);
            let mut push = move |mono: Vec<f32>| {
                let out = resampler.process(&mono);
                if let Ok(mut q) = cap.capture.lock() {
                    q.extend(out);
                    let excess = q.len().saturating_sub(SAMPLE_RATE / 2);
                    q.drain(..excess);
                }
            };
            let err = |e| log::warn!("micro: {e}");
            let stream = match cfg.sample_format() {
                cpal::SampleFormat::F32 => dev.build_input_stream(
                    &cfg.into(),
                    move |data: &[f32], _: &_| {
                        push(
                            data.chunks(channels)
                                .map(|c| c.iter().sum::<f32>() / channels as f32)
                                .collect(),
                        )
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::I16 => dev.build_input_stream(
                    &cfg.into(),
                    move |data: &[i16], _: &_| {
                        push(
                            data.chunks(channels)
                                .map(|c| {
                                    c.iter().map(|s| *s as f32 / 32768.).sum::<f32>()
                                        / channels as f32
                                })
                                .collect(),
                        )
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::U16 => dev.build_input_stream(
                    &cfg.into(),
                    move |data: &[u16], _: &_| {
                        push(
                            data.chunks(channels)
                                .map(|c| {
                                    c.iter().map(|s| (*s as f32 - 32768.) / 32768.).sum::<f32>()
                                        / channels as f32
                                })
                                .collect(),
                        )
                    },
                    err,
                    None,
                ),
                _ => return None,
            }
            .ok()?;
            stream.play().ok()?;
            Some(stream)
        });
        if input.is_none() {
            let _ = events.send(VoiceEvent::State("Aucun micro disponible".into()));
        }
        let output = find_device(false, &settings.output_device).and_then(|dev| {
            let cfg = dev.default_output_config().ok()?;
            let channels = cfg.channels() as usize;
            let rate = cfg.sample_rate().0 as f64;
            let play = shared.clone();
            let deaf = settings.deafened.clone();
            let mut mixer = Mixer::new(SAMPLE_RATE as f64 / rate);
            let err = |e| log::warn!("haut-parleur: {e}");
            let stream = match cfg.sample_format() {
                cpal::SampleFormat::F32 => dev.build_output_stream(
                    &cfg.into(),
                    move |out: &mut [f32], _: &_| {
                        mixer.fill(
                            &play.playback,
                            &deaf,
                            channels,
                            out.len() / channels,
                            |i, v| out[i] = v,
                        )
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::I16 => dev.build_output_stream(
                    &cfg.into(),
                    move |out: &mut [i16], _: &_| {
                        mixer.fill(
                            &play.playback,
                            &deaf,
                            channels,
                            out.len() / channels,
                            |i, v| out[i] = (v * 32767.) as i16,
                        )
                    },
                    err,
                    None,
                ),
                cpal::SampleFormat::U16 => dev.build_output_stream(
                    &cfg.into(),
                    move |out: &mut [u16], _: &_| {
                        mixer.fill(
                            &play.playback,
                            &deaf,
                            channels,
                            out.len() / channels,
                            |i, v| out[i] = ((v * 32767.) + 32768.) as u16,
                        )
                    },
                    err,
                    None,
                ),
                _ => return None,
            }
            .ok()?;
            stream.play().ok()?;
            Some(stream)
        });
        if output.is_none() {
            let _ = events.send(VoiceEvent::State("Aucune sortie audio disponible".into()));
        }
        while !shared.stop.load(Ordering::Relaxed) {
            std::thread::sleep(Duration::from_millis(100));
        }
        drop(input);
        drop(output);
    })
}

/// Linear resampler for mono f32 streams.
struct Resampler {
    step: f64,
    pos: f64,
    last: f32,
}

impl Resampler {
    fn new(from: f64, to: f64) -> Self {
        Self {
            step: from / to,
            pos: 0.,
            last: 0.,
        }
    }

    fn process(&mut self, input: &[f32]) -> Vec<f32> {
        if (self.step - 1.).abs() < 1e-9 {
            return input.to_vec();
        }
        let mut out = Vec::with_capacity((input.len() as f64 / self.step) as usize + 2);
        let at = |i: isize, last: f32| if i < 0 { last } else { input[i as usize] };
        while (self.pos as usize) < input.len() {
            let i = self.pos.floor() as isize - 1;
            let frac = (self.pos - self.pos.floor()) as f32;
            let a = at(i, self.last);
            let b = at(i + 1, self.last);
            out.push(a + (b - a) * frac);
            self.pos += self.step;
        }
        self.pos -= input.len() as f64;
        self.last = *input.last().unwrap_or(&self.last);
        out
    }
}

/// Mixes every speaker's queue into the output device, resampling on the fly.
struct Mixer {
    /// Source samples (48 kHz) consumed per output frame.
    step: f64,
    pos: f64,
    current: [f32; 2],
    next: [f32; 2],
}

impl Mixer {
    fn new(step: f64) -> Self {
        Self {
            step,
            pos: 1.,
            current: [0.; 2],
            next: [0.; 2],
        }
    }

    /// Pops one stereo frame (sum of all speakers) at 48 kHz.
    fn pull(play: &Mutex<HashMap<u32, VecDeque<f32>>>) -> [f32; 2] {
        let mut acc = [0f32; 2];
        if let Ok(mut map) = play.lock() {
            for q in map.values_mut() {
                if q.len() >= 2 {
                    acc[0] += q.pop_front().unwrap_or(0.);
                    acc[1] += q.pop_front().unwrap_or(0.);
                }
            }
        }
        [acc[0].clamp(-1., 1.), acc[1].clamp(-1., 1.)]
    }

    fn fill(
        &mut self,
        play: &Mutex<HashMap<u32, VecDeque<f32>>>,
        deaf: &AtomicBool,
        channels: usize,
        frames: usize,
        mut write: impl FnMut(usize, f32),
    ) {
        let deafened = deaf.load(Ordering::Relaxed);
        for f in 0..frames {
            while self.pos >= 1. {
                self.current = self.next;
                self.next = Self::pull(play);
                self.pos -= 1.;
            }
            let t = self.pos as f32;
            let l = self.current[0] + (self.next[0] - self.current[0]) * t;
            let r = self.current[1] + (self.next[1] - self.current[1]) * t;
            self.pos += self.step;
            for c in 0..channels {
                let v = if deafened {
                    0.
                } else if channels == 1 {
                    (l + r) * 0.5
                } else if c % 2 == 0 {
                    l
                } else {
                    r
                };
                write(f * channels + c, v);
            }
        }
    }
}

// ---- send / receive loops ---------------------------------------------------------------

fn send_loop(
    shared: Arc<Shared>,
    sock: Arc<UdpSocket>,
    settings: VoiceSettings,
    events: flume::Sender<VoiceEvent>,
) {
    let Ok(encoder) = Encoder::new(SampleRate::Hz48000, Channels::Stereo, Application::Voip) else {
        let _ = events.send(VoiceEvent::State("Encodeur Opus indisponible".into()));
        return;
    };
    let mut denoise = nnnoiseless::DenoiseState::new();
    let mut seq: u16 = rand::random();
    let mut timestamp: u32 = rand::random();
    let mut nonce: u32 = 0;
    let mut speaking = false;
    let mut hangover = 0u32;
    let mut silence_left = 0u32;
    let mut next = Instant::now();
    let mut opus = vec![0u8; 4000];

    while !shared.stop.load(Ordering::Relaxed) {
        next += Duration::from_millis(20);
        let now = Instant::now();
        if next > now {
            std::thread::sleep(next - now);
        } else {
            next = now;
        }

        // Take 20 ms of microphone audio.
        let mut mono: Vec<f32> = match shared.capture.lock() {
            Ok(mut q) if q.len() >= FRAME => q.drain(..FRAME).collect(),
            _ => continue,
        };
        let muted = settings.muted.load(Ordering::Relaxed);

        // Noise suppression (RNNoise works on 10 ms frames in i16 range) + VAD.
        let mut vad = 0f32;
        if settings.noise_suppression.load(Ordering::Relaxed) {
            let mut out = vec![0f32; FRAME];
            for k in 0..2 {
                let input: Vec<f32> = mono[k * 480..(k + 1) * 480]
                    .iter()
                    .map(|s| s * 32767.)
                    .collect();
                vad = vad.max(denoise.process_frame(&mut out[k * 480..(k + 1) * 480], &input));
            }
            mono = out.iter().map(|s| s / 32767.).collect();
        }
        let rms = (mono.iter().map(|s| s * s).sum::<f32>() / FRAME as f32).sqrt();
        let voice = !muted
            && if settings.noise_suppression.load(Ordering::Relaxed) {
                vad > 0.6 && rms > 0.004
            } else {
                rms > 0.01
            };
        if voice {
            hangover = 15; // keep transmitting 300 ms after speech
        } else {
            hangover = hangover.saturating_sub(1);
        }
        let transmit = hangover > 0;

        if transmit != speaking {
            speaking = transmit;
            if let Ok(mut s) = shared.speaking_changed.lock() {
                *s = Some(speaking);
            }
            let _ = events.send(VoiceEvent::Speaking {
                user_id: String::new(),
                speaking,
            });
            if !speaking {
                silence_left = 5;
            }
        }

        let packet: Vec<u8> = if transmit {
            let stereo: Vec<i16> = mono
                .iter()
                .flat_map(|s| {
                    let v = (s.clamp(-1., 1.) * 32767.) as i16;
                    [v, v]
                })
                .collect();
            match encoder.encode(&stereo, &mut opus) {
                Ok(n) => opus[..n].to_vec(),
                Err(_) => continue,
            }
        } else if silence_left > 0 {
            silence_left -= 1;
            OPUS_SILENCE.to_vec()
        } else {
            continue;
        };

        // End-to-end encryption (DAVE) when the call uses it.
        let payload = if shared.dave_active.load(Ordering::Relaxed) {
            let enc = shared.dave.lock().ok().and_then(|mut g| {
                g.as_mut()?
                    .encrypt_opus(&packet)
                    .ok()
                    .map(|c| c.into_owned())
            });
            match enc {
                Some(p) => p,
                None => continue,
            }
        } else {
            packet
        };

        let ssrc = shared.ssrc.load(Ordering::Relaxed);
        let mut header = [0u8; 12];
        header[0] = 0x80;
        header[1] = PAYLOAD_TYPE;
        header[2..4].copy_from_slice(&seq.to_be_bytes());
        header[4..8].copy_from_slice(&timestamp.to_be_bytes());
        header[8..12].copy_from_slice(&ssrc.to_be_bytes());
        seq = seq.wrapping_add(1);
        timestamp = timestamp.wrapping_add(FRAME as u32);

        let sealed = shared
            .cipher
            .lock()
            .ok()
            .and_then(|c| c.as_ref()?.seal(nonce, &header, &payload));
        let Some(sealed) = sealed else { continue };
        let mut pkt = Vec::with_capacity(12 + sealed.len() + 4);
        pkt.extend_from_slice(&header);
        pkt.extend_from_slice(&sealed);
        pkt.extend_from_slice(&nonce.to_be_bytes());
        nonce = nonce.wrapping_add(1);
        let _ = sock.send(&pkt);
    }
}

fn receive_loop(
    shared: Arc<Shared>,
    sock: Arc<UdpSocket>,
    settings: VoiceSettings,
    events: flume::Sender<VoiceEvent>,
) {
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));
    let mut decoders: HashMap<u32, Decoder> = HashMap::new();
    let mut last_voice: HashMap<u32, Instant> = HashMap::new();
    let mut announced: HashSet<u32> = HashSet::new();
    let mut buf = vec![0u8; 4096];
    let mut pcm = vec![0i16; FRAME * 2 * 3];

    while !shared.stop.load(Ordering::Relaxed) {
        // Speaking timeouts for the UI.
        let now = Instant::now();
        announced.retain(|ssrc| {
            let alive = last_voice
                .get(ssrc)
                .is_some_and(|t| now.duration_since(*t) < Duration::from_millis(350));
            if !alive {
                if let Some(uid) = shared.users.lock().ok().and_then(|m| m.get(ssrc).copied()) {
                    let _ = events.send(VoiceEvent::Speaking {
                        user_id: uid.to_string(),
                        speaking: false,
                    });
                }
            }
            alive
        });

        let n = match sock.recv(&mut buf) {
            Ok(n) => n,
            Err(_) => continue,
        };
        let pkt = &buf[..n];
        // RTP only (skip RTCP 200..=206 and control packets).
        if n < 12 + 4 + 16
            || pkt[0] >> 6 != 2
            || (200..=206).contains(&pkt[1])
            || pkt[1] & 0x7f != PAYLOAD_TYPE
        {
            continue;
        }
        let cc = (pkt[0] & 0x0f) as usize;
        let has_ext = pkt[0] & 0x10 != 0;
        let mut hdr = 12 + cc * 4;
        let mut ext_words = 0usize;
        if has_ext {
            if n < hdr + 4 {
                continue;
            }
            ext_words = u16::from_be_bytes([pkt[hdr + 2], pkt[hdr + 3]]) as usize;
            hdr += 4;
        }
        let ssrc = u32::from_be_bytes([pkt[8], pkt[9], pkt[10], pkt[11]]);
        if ssrc == shared.ssrc.load(Ordering::Relaxed) || settings.deafened.load(Ordering::Relaxed)
        {
            continue;
        }
        let plain = shared.cipher.lock().ok().and_then(|c| {
            c.as_ref()?
                .open(&pkt[n - 4..], &pkt[..hdr], &pkt[hdr..n - 4])
        });
        let Some(plain) = plain else { continue };
        if plain.len() < ext_words * 4 {
            continue;
        }
        let mut opus = plain[ext_words * 4..].to_vec();
        if opus == OPUS_SILENCE {
            continue;
        }

        let user = shared.users.lock().ok().and_then(|m| m.get(&ssrc).copied());
        // DAVE frames end with the 0xFAFA magic marker.
        if opus.len() > 2 && opus[opus.len() - 2..] == [0xFA, 0xFA] {
            let Some(uid) = user else { continue };
            let dec = shared
                .dave
                .lock()
                .ok()
                .and_then(|mut g| g.as_mut()?.decrypt(uid, MediaType::AUDIO, &opus).ok());
            match dec {
                Some(d) => opus = d,
                None => continue,
            }
        }

        let dec = decoders.entry(ssrc).or_insert_with(|| {
            Decoder::new(SampleRate::Hz48000, Channels::Stereo).expect("opus decoder")
        });
        let Ok(packet) = audiopus::packet::Packet::try_from(opus.as_slice()) else {
            continue;
        };
        let Ok(signals) = MutSignals::try_from(pcm.as_mut_slice()) else {
            continue;
        };
        let Ok(frames) = dec.decode(Some(packet), signals, false) else {
            continue;
        };

        if let Ok(mut map) = shared.playback.lock() {
            let q = map.entry(ssrc).or_default();
            q.extend(pcm[..frames * 2].iter().map(|s| *s as f32 / 32768.));
            // Bound latency / memory: keep at most 400 ms per speaker.
            let excess = q.len().saturating_sub(SAMPLE_RATE * 2 * 2 / 5);
            q.drain(..excess);
        }

        last_voice.insert(ssrc, Instant::now());
        if announced.insert(ssrc) {
            if let Some(uid) = user {
                let _ = events.send(VoiceEvent::Speaking {
                    user_id: uid.to_string(),
                    speaking: true,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transport_roundtrip_both_modes() {
        let key = [7u8; 32];
        for mode in ["aead_aes256_gcm_rtpsize", "aead_xchacha20_poly1305_rtpsize"] {
            let c = Cipher::new(mode, &key).unwrap();
            let header = [0x80, PAYLOAD_TYPE, 0, 1, 0, 0, 3, 192, 0, 0, 0, 42];
            let sealed = c.seal(5, &header, b"opus-frame").unwrap();
            let opened = c.open(&5u32.to_be_bytes(), &header, &sealed).unwrap();
            assert_eq!(opened, b"opus-frame");
            // Tampered header must fail authentication.
            let mut bad = header;
            bad[11] = 43;
            assert!(c.open(&5u32.to_be_bytes(), &bad, &sealed).is_none());
        }
    }

    #[test]
    fn resampler_keeps_rate() {
        let mut r = Resampler::new(44_100., 48_000.);
        let out = r.process(&vec![0.5; 4410]);
        assert!((out.len() as i64 - 4800).abs() <= 2, "{}", out.len());
        assert!(out.iter().skip(2).all(|s| (s - 0.5).abs() < 1e-6));
    }

    #[test]
    fn opus_and_dave_silence() {
        let enc = Encoder::new(SampleRate::Hz48000, Channels::Stereo, Application::Voip).unwrap();
        let mut out = vec![0u8; 4000];
        let n = enc.encode(&vec![0i16; FRAME * 2], &mut out).unwrap();
        assert!(n > 0);
        let mut dec = Decoder::new(SampleRate::Hz48000, Channels::Stereo).unwrap();
        let mut pcm = vec![0i16; FRAME * 2];
        let frames = dec
            .decode(
                Some(audiopus::packet::Packet::try_from(&out[..n]).unwrap()),
                MutSignals::try_from(pcm.as_mut_slice()).unwrap(),
                false,
            )
            .unwrap();
        assert_eq!(frames, FRAME);
        assert_eq!(davey::OPUS_SILENCE_PACKET, OPUS_SILENCE);
    }

    #[test]
    fn denoiser_runs() {
        let mut d = nnnoiseless::DenoiseState::new();
        let mut out = [0f32; 480];
        let vad = d.process_frame(&mut out, &[0f32; 480]);
        assert!((0.0..=1.0).contains(&vad));
    }
}
