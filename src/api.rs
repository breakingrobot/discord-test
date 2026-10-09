//! Blocking Discord REST helpers. Call these from a background executor.

use std::io::Read;

use base64::Engine;
use serde::Deserialize;
use serde_json::{json, Value};

const BASE: &str = "https://discord.com/api/v9";

/// Mimic the official web client (what Abaddon/Dissent do) to stay out of the spam filter.
const USER_AGENT: &str = "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36";
const BUILD_NUMBER: u32 = 350_000;

#[derive(Clone, Debug, Default, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub global_name: Option<String>,
    #[serde(default)]
    pub avatar: Option<String>,
}

impl User {
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }

    pub fn avatar_url(&self) -> Option<String> {
        self.avatar.as_ref().map(|h| {
            format!(
                "https://cdn.discordapp.com/avatars/{}/{h}.png?size=64",
                self.id
            )
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Guild {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub icon: Option<String>,
}

impl Guild {
    pub fn icon_url(&self) -> Option<String> {
        self.icon.as_ref().map(|h| {
            format!(
                "https://cdn.discordapp.com/icons/{}/{h}.png?size=96",
                self.id
            )
        })
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: Option<String>,
    /// 0 text, 1 DM, 3 group DM, 4 category, 5 announcement.
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub position: i32,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub topic: Option<String>,
    #[serde(default)]
    pub recipients: Vec<User>,
}

impl Channel {
    pub fn is_dm(&self) -> bool {
        self.kind == 1 || self.kind == 3
    }

    pub fn title(&self) -> String {
        match &self.name {
            Some(n) if !n.is_empty() => n.clone(),
            _ => self
                .recipients
                .iter()
                .map(|r| r.display_name())
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Attachment {
    pub filename: String,
    #[serde(default)]
    pub content_type: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub proxy_url: Option<String>,
    #[serde(default)]
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
}

impl Attachment {
    pub fn is_image(&self) -> bool {
        self.content_type
            .as_deref()
            .is_some_and(|t| t.starts_with("image/"))
    }

    /// Small PNG rendition served by Discord's media proxy (always decodable).
    pub fn preview_url(&self) -> String {
        let base = self.proxy_url.as_deref().unwrap_or(&self.url);
        format!("{base}?format=png&width=400&height=300")
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct EmbedMedia {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub proxy_url: Option<String>,
}

impl EmbedMedia {
    pub fn preview_url(&self) -> String {
        let base = self.proxy_url.as_deref().unwrap_or(&self.url);
        format!("{base}?format=png&width=400&height=300")
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct EmbedField {
    pub name: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct EmbedAuthor {
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Embed {
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub color: Option<u32>,
    #[serde(default)]
    pub author: Option<EmbedAuthor>,
    #[serde(default)]
    pub fields: Vec<EmbedField>,
    #[serde(default)]
    pub image: Option<EmbedMedia>,
    #[serde(default)]
    pub thumbnail: Option<EmbedMedia>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Emoji {
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub name: Option<String>,
}

impl Emoji {
    pub fn label(&self) -> String {
        self.name.clone().unwrap_or_default()
    }

    /// Path segment for the reactions endpoints.
    pub fn api_key(&self) -> String {
        match &self.id {
            Some(id) => format!("{}:{id}", self.name.clone().unwrap_or_default()),
            None => self.name.clone().unwrap_or_default(),
        }
    }

    pub fn url(&self) -> Option<String> {
        self.id
            .as_ref()
            .map(|id| format!("https://cdn.discordapp.com/emojis/{id}.png?size=32"))
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Reaction {
    pub count: u32,
    #[serde(default)]
    pub me: bool,
    pub emoji: Emoji,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Message {
    pub id: String,
    #[serde(default)]
    pub channel_id: String,
    #[serde(default)]
    pub guild_id: Option<String>,
    #[serde(default)]
    pub mention_everyone: bool,
    pub content: String,
    pub author: User,
    /// RFC 3339.
    pub timestamp: String,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub embeds: Vec<Embed>,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
    #[serde(default)]
    pub mentions: Vec<User>,
    #[serde(default)]
    pub referenced_message: Option<Box<Message>>,
}

/// Client fingerprint sent both as a header and in the gateway IDENTIFY.
pub fn super_properties() -> Value {
    json!({
        "os": "Windows",
        "browser": "Chrome",
        "device": "",
        "system_locale": "fr-FR",
        "browser_user_agent": USER_AGENT,
        "browser_version": "131.0.0.0",
        "os_version": "10",
        "referrer": "",
        "referring_domain": "",
        "referrer_current": "",
        "referring_domain_current": "",
        "release_channel": "stable",
        "client_build_number": BUILD_NUMBER,
        "client_event_source": null,
    })
}

fn request(method: &str, path: &str, auth: Option<&str>) -> ureq::Request {
    let props = base64::engine::general_purpose::STANDARD.encode(super_properties().to_string());
    let mut r = ureq::request(method, &format!("{BASE}{path}"))
        .set("User-Agent", USER_AGENT)
        .set("X-Super-Properties", &props)
        .set("X-Discord-Locale", "fr")
        .set("Accept-Language", "fr-FR,fr;q=0.9")
        .set("Origin", "https://discord.com")
        .set("Referer", "https://discord.com/channels/@me");
    if let Some(a) = auth {
        r = r.set("Authorization", a);
    }
    r
}

/// Human-readable message out of a Discord error response.
fn describe(e: ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, resp) => {
            let body: Value = resp.into_json().unwrap_or(Value::Null);
            match body["message"].as_str() {
                Some(m) => format!("{m} ({code})"),
                None => format!("HTTP {code}"),
            }
        }
        other => other.to_string(),
    }
}

fn get<T: for<'de> Deserialize<'de>>(token: &str, path: &str) -> Result<T, String> {
    request("GET", path, Some(token))
        .call()
        .map_err(describe)?
        .into_json()
        .map_err(|e| e.to_string())
}

fn send_json(method: &str, token: &str, path: &str, body: Value) -> Result<ureq::Response, String> {
    request(method, path, Some(token))
        .send_json(body)
        .map_err(describe)
}

/// Validates a token. Tries it as a user token first, then as a bot token.
/// Returns the working `Authorization` value and the account.
pub fn login(token: &str) -> Result<(String, User), String> {
    match get::<User>(token, "/users/@me") {
        Ok(u) => Ok((token.to_string(), u)),
        Err(first) => {
            let bot = format!("Bot {token}");
            get::<User>(&bot, "/users/@me")
                .map(|u| (bot, u))
                .map_err(|_| first)
        }
    }
}

pub enum PasswordLogin {
    Token(String),
    /// Two-factor code required; carries the MFA ticket.
    Mfa(String),
    Captcha,
}

/// E-mail/phone + password sign-in. Discord frequently demands a captcha here.
pub fn password_login(login: &str, password: &str) -> Result<PasswordLogin, String> {
    let body = json!({
        "login": login, "password": password, "undelete": false,
        "login_source": null, "gift_code_sku_id": null,
    });
    match request("POST", "/auth/login", None).send_json(body) {
        Ok(resp) => {
            let v: Value = resp.into_json().map_err(|e| e.to_string())?;
            if let Some(t) = v["token"].as_str() {
                Ok(PasswordLogin::Token(t.to_string()))
            } else if let Some(t) = v["ticket"].as_str() {
                Ok(PasswordLogin::Mfa(t.to_string()))
            } else {
                Err("Réponse inattendue de Discord".into())
            }
        }
        Err(ureq::Error::Status(_, resp)) => {
            let v: Value = resp.into_json().unwrap_or(Value::Null);
            if v.get("captcha_key").is_some() {
                Ok(PasswordLogin::Captcha)
            } else if let Some(errs) = v["errors"].as_object() {
                let _ = errs;
                Err("E-mail ou mot de passe invalide.".into())
            } else {
                Err(v["message"]
                    .as_str()
                    .unwrap_or("Connexion refusée")
                    .to_string())
            }
        }
        Err(e) => Err(e.to_string()),
    }
}

pub fn mfa_totp(code: &str, ticket: &str) -> Result<String, String> {
    let v: Value = request("POST", "/auth/mfa/totp", None)
        .send_json(json!({ "code": code, "ticket": ticket, "login_source": null, "gift_code_sku_id": null }))
        .map_err(describe)?
        .into_json()
        .map_err(|e| e.to_string())?;
    v["token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Code invalide".into())
}

/// Second step of QR login: exchanges the scanned ticket for an encrypted token.
pub fn remote_auth_login(ticket: &str) -> Result<String, String> {
    let v: Value = request("POST", "/users/@me/remote-auth/login", None)
        .send_json(json!({ "ticket": ticket }))
        .map_err(describe)?
        .into_json()
        .map_err(|e| e.to_string())?;
    v["encrypted_token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| "Réponse inattendue de Discord".into())
}

pub fn guilds(token: &str) -> Result<Vec<Guild>, String> {
    get(token, "/users/@me/guilds")
}

/// Text/announcement channels and categories, in Discord's display order.
pub fn channels(token: &str, guild: &str) -> Result<Vec<Channel>, String> {
    let mut chans: Vec<Channel> = get(token, &format!("/guilds/{guild}/channels"))?;
    chans.retain(|c| matches!(c.kind, 0 | 4 | 5));
    chans.sort_by_key(|c| c.position);
    Ok(chans)
}

/// Open DMs and group DMs, most recently active first.
pub fn dms(token: &str) -> Result<Vec<Channel>, String> {
    let mut chans: Vec<Channel> = get(token, "/users/@me/channels")?;
    // Channel ids are snowflakes; for DMs the id order tracks last activity closely enough.
    chans.sort_by(|a, b| b.id.len().cmp(&a.id.len()).then(b.id.cmp(&a.id)));
    Ok(chans)
}

#[derive(Clone, Debug, Deserialize)]
pub struct Relationship {
    #[serde(rename = "type")]
    pub kind: u8,
    pub user: User,
}

/// Friends (type 1) sorted by name.
pub fn friends(token: &str) -> Result<Vec<User>, String> {
    let rels: Vec<Relationship> = get(token, "/users/@me/relationships")?;
    let mut users: Vec<User> = rels
        .into_iter()
        .filter(|r| r.kind == 1)
        .map(|r| r.user)
        .collect();
    users.sort_by_key(|u| u.display_name().to_lowercase());
    Ok(users)
}

pub fn open_dm(token: &str, user_id: &str) -> Result<Channel, String> {
    send_json(
        "POST",
        token,
        "/users/@me/channels",
        json!({ "recipient_id": user_id }),
    )?
    .into_json()
    .map_err(|e| e.to_string())
}

/// Oldest first. `before` pages backwards from a message id.
pub fn messages(token: &str, channel: &str, before: Option<&str>) -> Result<Vec<Message>, String> {
    let mut path = format!("/channels/{channel}/messages?limit=50");
    if let Some(b) = before {
        path.push_str(&format!("&before={b}"));
    }
    let mut msgs: Vec<Message> = get(token, &path)?;
    msgs.reverse();
    Ok(msgs)
}

fn nonce() -> String {
    // Discord expects a snowflake-looking nonce; time-based is enough.
    let ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    (((ms.saturating_sub(1_420_070_400_000)) << 22) | 7).to_string()
}

pub fn send(
    token: &str,
    channel: &str,
    content: &str,
    reply_to: Option<&str>,
) -> Result<(), String> {
    let mut body = json!({ "content": content, "nonce": nonce(), "tts": false });
    if let Some(id) = reply_to {
        body["message_reference"] = json!({ "message_id": id });
    }
    send_json(
        "POST",
        token,
        &format!("/channels/{channel}/messages"),
        body,
    )?;
    Ok(())
}

/// Sends files (multipart) with optional text.
pub fn upload(
    token: &str,
    channel: &str,
    content: &str,
    files: &[std::path::PathBuf],
) -> Result<(), String> {
    let boundary = format!("----discordgpui{}", nonce());
    let mut body: Vec<u8> = Vec::new();
    let names: Vec<String> = files
        .iter()
        .map(|p| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| "fichier".into())
        })
        .collect();
    let attachments: Vec<Value> = names
        .iter()
        .enumerate()
        .map(|(i, n)| json!({ "id": i, "filename": n }))
        .collect();
    let payload = json!({ "content": content, "nonce": nonce(), "attachments": attachments });
    body.extend(
        format!(
            "--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\nContent-Type: application/json\r\n\r\n{payload}\r\n"
        )
        .as_bytes(),
    );
    for (i, (path, name)) in files.iter().zip(&names).enumerate() {
        let data = std::fs::read(path).map_err(|e| format!("{name} : {e}"))?;
        body.extend(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"files[{i}]\"; filename=\"{}\"\r\nContent-Type: application/octet-stream\r\n\r\n",
                name.replace('"', "")
            )
            .as_bytes(),
        );
        body.extend(data);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{boundary}--\r\n").as_bytes());
    request(
        "POST",
        &format!("/channels/{channel}/messages"),
        Some(token),
    )
    .set(
        "Content-Type",
        &format!("multipart/form-data; boundary={boundary}"),
    )
    .send_bytes(&body)
    .map_err(describe)?;
    Ok(())
}

pub fn edit(token: &str, channel: &str, message: &str, content: &str) -> Result<(), String> {
    send_json(
        "PATCH",
        token,
        &format!("/channels/{channel}/messages/{message}"),
        json!({ "content": content }),
    )?;
    Ok(())
}

pub fn delete(token: &str, channel: &str, message: &str) -> Result<(), String> {
    request(
        "DELETE",
        &format!("/channels/{channel}/messages/{message}"),
        Some(token),
    )
    .call()
    .map_err(describe)?;
    Ok(())
}

pub fn typing(token: &str, channel: &str) -> Result<(), String> {
    request("POST", &format!("/channels/{channel}/typing"), Some(token))
        .call()
        .map_err(describe)?;
    Ok(())
}

/// Marks a channel as read up to a message (so other devices clear their badges).
pub fn ack(token: &str, channel: &str, message: &str) -> Result<(), String> {
    send_json(
        "POST",
        token,
        &format!("/channels/{channel}/messages/{message}/ack"),
        json!({ "token": null }),
    )?;
    Ok(())
}

fn percent_encode(s: &str) -> String {
    s.bytes()
        .map(|b| match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                (b as char).to_string()
            }
            _ => format!("%{b:02X}"),
        })
        .collect()
}

/// Adds or removes our own reaction.
pub fn react(
    token: &str,
    channel: &str,
    message: &str,
    emoji: &Emoji,
    add: bool,
) -> Result<(), String> {
    let path = format!(
        "/channels/{channel}/messages/{message}/reactions/{}/@me",
        percent_encode(&emoji.api_key())
    );
    request(if add { "PUT" } else { "DELETE" }, &path, Some(token))
        .set("Content-Length", "0")
        .call()
        .map_err(describe)?;
    Ok(())
}

/// Downloads an image from the CDN (capped at 8 MiB).
pub fn fetch_image(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url)
        .set("User-Agent", USER_AGENT)
        .call()
        .map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
