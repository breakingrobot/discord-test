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
    #[serde(default)]
    pub banner: Option<String>,
    #[serde(default)]
    pub accent_color: Option<u32>,
    #[serde(default)]
    pub bot: bool,
    #[serde(default)]
    pub public_flags: u64,
    /// Guild tag ("clan") shown next to the name.
    #[serde(default)]
    pub primary_guild: Option<PrimaryGuild>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct PrimaryGuild {
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub identity_enabled: Option<bool>,
}

impl User {
    /// Guild tag when the user displays one.
    pub fn tag(&self) -> Option<&str> {
        let g = self.primary_guild.as_ref()?;
        if g.identity_enabled == Some(false) {
            return None;
        }
        g.tag.as_deref().filter(|t| !t.is_empty())
    }

    pub fn banner_url(&self) -> Option<String> {
        self.banner.as_ref().map(|h| {
            format!(
                "https://cdn.discordapp.com/banners/{}/{h}.png?size=480",
                self.id
            )
        })
    }

    /// Account creation time (ms since epoch), derived from the snowflake.
    pub fn created_ms(&self) -> u64 {
        (self.id.parse::<u64>().unwrap_or(0) >> 22) + 1_420_070_400_000
    }

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
                "https://cdn.discordapp.com/icons/{}/{h}.png?size=80",
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
    #[serde(default)]
    pub message_count: Option<u32>,
    #[serde(default)]
    pub thread_metadata: Option<ThreadMeta>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ThreadMeta {
    #[serde(default)]
    pub archived: bool,
}

impl Channel {
    pub fn is_thread(&self) -> bool {
        matches!(self.kind, 10..=12)
    }

    /// Forum / media channels list posts (threads) instead of messages.
    pub fn is_voice(&self) -> bool {
        matches!(self.kind, 2 | 13)
    }

    pub fn is_forum(&self) -> bool {
        matches!(self.kind, 15 | 16)
    }

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
    pub fn is_gif(&self) -> bool {
        self.content_type.as_deref() == Some("image/gif")
            || self.filename.to_lowercase().ends_with(".gif")
    }

    /// Size-capped rendition for the chat; GIFs keep their animation.
    pub fn display_url(&self) -> String {
        let base = self.proxy_url.as_deref().unwrap_or(&self.url);
        if self.is_gif() {
            format!("{base}?width=400&height=300")
        } else {
            self.preview_url()
        }
    }

    /// Big rendition for the image viewer.
    pub fn large_url(&self) -> String {
        let base = self.proxy_url.as_deref().unwrap_or(&self.url);
        if self.is_gif() {
            base.to_string()
        } else {
            format!("{base}?format=png&width=1600&height=1200")
        }
    }

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
    pub width: Option<u32>,
    #[serde(default)]
    pub height: Option<u32>,
    #[serde(default)]
    pub proxy_url: Option<String>,
}

impl EmbedMedia {
    /// Animated rendition for Tenor "gifv" embeds (their MP4 has a GIF twin).
    pub fn tenor_gif(&self) -> Option<String> {
        let u = &self.url;
        (u.contains("media.tenor.com") && u.ends_with(".mp4"))
            .then(|| u.replace("AAAPo/", "AAAAC/").replace(".mp4", ".gif"))
    }

    /// Large rendition (keeps animation: no format conversion).
    pub fn large_url(&self) -> String {
        let base = self.proxy_url.as_deref().unwrap_or(&self.url);
        format!(
            "{base}{}width=1280&height=960",
            if base.contains('?') { "&" } else { "?" }
        )
    }

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
    /// "rich", "image", "gifv", "video", "link", "article"…
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub video: Option<EmbedMedia>,
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
    /// Present on gateway events in guilds.
    #[serde(default)]
    pub member: Option<MemberPart>,
    /// Thread started from this message.
    #[serde(default)]
    pub thread: Option<Box<Channel>>,
    /// 0 default, 19 reply, 6 pin, 7 join, 18 thread created, 21 thread starter, …
    #[serde(rename = "type", default)]
    pub kind: u8,
    #[serde(default)]
    pub message_reference: Option<MessageRef>,
    #[serde(default)]
    pub mention_roles: Vec<String>,
    #[serde(default)]
    pub sticker_items: Vec<Sticker>,
    #[serde(default)]
    pub pinned: bool,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MessageRef {
    #[serde(default)]
    pub message_id: Option<String>,
    #[serde(default)]
    pub channel_id: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Sticker {
    pub id: String,
    pub name: String,
    /// 1 PNG, 2 APNG, 3 Lottie, 4 GIF.
    #[serde(default)]
    pub format_type: u8,
}

impl Sticker {
    pub fn url(&self) -> Option<String> {
        matches!(self.format_type, 1 | 2 | 4).then(|| {
            format!(
                "https://media.discordapp.net/stickers/{}.png?size=160",
                self.id
            )
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct MemberPart {
    #[serde(default)]
    pub roles: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Role {
    pub name: String,
    pub color: u32,
    pub position: i64,
}

#[derive(Clone, Debug)]
pub struct Badge {
    pub description: String,
    pub icon: String,
}

impl Badge {
    pub fn url(&self) -> String {
        format!("https://cdn.discordapp.com/badge-icons/{}.png", self.icon)
    }
}

#[derive(Clone, Debug)]
pub struct Connection {
    pub kind: String,
    pub name: String,
    pub verified: bool,
}

/// Full profile as returned by `GET /users/{id}/profile`.
#[derive(Clone, Debug, Default)]
pub struct Profile {
    pub user: User,
    pub bio: Option<String>,
    pub pronouns: Option<String>,
    pub accent_color: Option<u32>,
    pub badges: Vec<Badge>,
    pub connections: Vec<Connection>,
    /// (guild id, nickname)
    pub mutual_guilds: Vec<(String, Option<String>)>,
    pub mutual_friends_count: u32,
    pub premium_since: Option<String>,
    pub premium_guild_since: Option<String>,
    pub joined_at: Option<String>,
    pub nick: Option<String>,
    pub member_roles: Vec<String>,
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
            match code {
                401 => "Session invalide ou expirée (401)".to_string(),
                403 => "Vous n'avez pas la permission pour cette action (403)".to_string(),
                404 => "Introuvable (404)".to_string(),
                429 => format!(
                    "Trop de requêtes, réessayez dans {} s (429)",
                    body["retry_after"].as_f64().unwrap_or(5.0).ceil()
                ),
                _ => match body["message"].as_str() {
                    Some(m) => format!("{m} ({code})"),
                    None => format!("HTTP {code}"),
                },
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
    chans.retain(|c| matches!(c.kind, 0 | 2 | 4 | 5 | 13 | 15 | 16));
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

#[derive(Clone, Debug, Deserialize)]
pub struct GuildEmoji {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub animated: bool,
}

impl GuildEmoji {
    pub fn url(&self) -> String {
        format!("https://cdn.discordapp.com/emojis/{}.png?size=48", self.id)
    }

    /// Markup inserted in the composer.
    pub fn markup(&self) -> String {
        format!(
            "<{}:{}:{}>",
            if self.animated { "a" } else { "" },
            self.name,
            self.id
        )
    }
}

pub fn guild_emojis(token: &str, guild: &str) -> Result<Vec<GuildEmoji>, String> {
    get(token, &format!("/guilds/{guild}/emojis"))
}

/// Role id -> name, used to label member-list groups.
pub fn roles(token: &str, guild: &str) -> Result<std::collections::HashMap<String, Role>, String> {
    let v: Vec<Value> = get(token, &format!("/guilds/{guild}/roles"))?;
    Ok(v.into_iter()
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
        .collect())
}

pub fn pins(token: &str, channel: &str) -> Result<Vec<Message>, String> {
    let mut msgs: Vec<Message> = get(token, &format!("/channels/{channel}/pins"))?;
    msgs.sort_by_key(|m| std::cmp::Reverse(m.id.parse::<u64>().unwrap_or(0)));
    Ok(msgs)
}

/// Full-text search in a guild (`guild = Some`) or a DM channel.
pub fn search(
    token: &str,
    guild: Option<&str>,
    channel: &str,
    query: &str,
) -> Result<Vec<Message>, String> {
    let q = percent_encode(query);
    let path = match guild {
        Some(g) => format!("/guilds/{g}/messages/search?content={q}"),
        None => format!("/channels/{channel}/messages/search?content={q}"),
    };
    let v: Value = get(token, &path)?;
    let groups = v["messages"]
        .as_array()
        .ok_or("L'index de recherche se prépare, réessayez dans un instant.")?;
    Ok(groups
        .iter()
        .filter_map(|g| {
            let g = g.as_array()?;
            let hit = g.iter().find(|m| m["hit"] == true).or(g.first())?;
            serde_json::from_value(hit.clone()).ok()
        })
        .collect())
}

#[derive(Clone, Debug, Deserialize)]
pub struct SlashOption {
    pub name: String,
    #[serde(default)]
    pub description: String,
    /// 3 string, 4 integer, 5 boolean, 10 number; 1/2 are sub-commands.
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub required: bool,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SlashCommand {
    pub id: String,
    pub application_id: String,
    #[serde(default)]
    pub version: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(rename = "type", default = "chat_input")]
    pub kind: u8,
    #[serde(default)]
    pub options: Vec<SlashOption>,
}

fn chat_input() -> u8 {
    1
}

/// Application commands available in a channel (+ application names).
pub fn command_index(
    token: &str,
    channel: &str,
) -> Result<(Vec<SlashCommand>, std::collections::HashMap<String, String>), String> {
    let v: Value = get(
        token,
        &format!("/channels/{channel}/application-command-index"),
    )?;
    let cmds: Vec<SlashCommand> = v["application_commands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| serde_json::from_value(c.clone()).ok())
        .filter(|c: &SlashCommand| c.kind == 1)
        .collect();
    let apps = v["applications"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|a| {
            Some((
                a["id"].as_str()?.to_string(),
                a["name"].as_str()?.to_string(),
            ))
        })
        .collect();
    Ok((cmds, apps))
}

/// Runs a slash command (the reply arrives as a normal gateway message).
pub fn run_command(
    token: &str,
    session_id: &str,
    guild: Option<&str>,
    channel: &str,
    cmd: &SlashCommand,
    options: Vec<Value>,
) -> Result<(), String> {
    let declared: Vec<Value> = cmd
        .options
        .iter()
        .map(|o| json!({ "type": o.kind, "name": o.name, "description": o.description, "required": o.required }))
        .collect();
    let mut payload = json!({
        "type": 2,
        "application_id": cmd.application_id,
        "channel_id": channel,
        "session_id": session_id,
        "data": {
            "version": cmd.version, "id": cmd.id, "name": cmd.name, "type": 1,
            "options": options,
            "application_command": {
                "id": cmd.id, "application_id": cmd.application_id, "version": cmd.version,
                "type": 1, "name": cmd.name, "description": cmd.description,
                "options": declared, "dm_permission": true, "nsfw": false,
            },
            "attachments": [],
        },
        "nonce": nonce(),
        "analytics_location": "slash_ui",
    });
    if let Some(g) = guild {
        payload["guild_id"] = json!(g);
    }
    let boundary = format!("----discordgpui{}", nonce());
    let body = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"payload_json\"\r\n\r\n{payload}\r\n--{boundary}--\r\n"
    );
    request("POST", "/interactions", Some(token))
        .set(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send_bytes(body.as_bytes())
        .map_err(describe)?;
    Ok(())
}

#[derive(Clone, Debug)]
pub struct Gif {
    pub title: String,
    /// Page / media URL posted into the chat (Discord unfurls it).
    pub url: String,
    /// Still image for the grid.
    pub preview: String,
    /// Animated media (GIF) and its size, for favourites.
    pub src: String,
    pub width: u32,
    pub height: u32,
}

/// Trending (empty query) or searched GIFs from Discord's Tenor proxy.
pub fn gifs(token: &str, query: &str) -> Result<Vec<Gif>, String> {
    let path = if query.trim().is_empty() {
        "/gifs/trending-gifs?provider=tenor&locale=fr&limit=30&media_format=gif".to_string()
    } else {
        format!(
            "/gifs/search?q={}&provider=tenor&locale=fr&limit=30&media_format=gif",
            percent_encode(query.trim())
        )
    };
    let v: Value = get(token, &path)?;
    let list = v
        .as_array()
        .or_else(|| v["gifs"].as_array())
        .ok_or("Réponse GIF inattendue")?;
    Ok(list
        .iter()
        .filter_map(|g| {
            let url = g["url"]
                .as_str()
                .or_else(|| g["gif_src"].as_str())?
                .to_string();
            let still = |k: &str| {
                g[k].as_str()
                    .filter(|u| {
                        let u = u.split('?').next().unwrap_or(u);
                        [".png", ".jpg", ".jpeg", ".gif", ".webp"]
                            .iter()
                            .any(|e| u.ends_with(e))
                    })
                    .map(str::to_string)
            };
            let preview = still("preview").or_else(|| still("gif_src"))?;
            Some(Gif {
                title: g["title"].as_str().unwrap_or("GIF").to_string(),
                src: g["gif_src"]
                    .as_str()
                    .or_else(|| g["src"].as_str())
                    .unwrap_or(&preview)
                    .to_string(),
                width: g["width"].as_u64().unwrap_or(0) as u32,
                height: g["height"].as_u64().unwrap_or(0) as u32,
                url,
                preview,
            })
        })
        .collect())
}

fn threads_of(v: Value) -> Vec<Channel> {
    v["threads"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|t| serde_json::from_value(t.clone()).ok())
        .collect()
}

pub fn active_threads(token: &str, guild: &str) -> Result<Vec<Channel>, String> {
    Ok(threads_of(get(
        token,
        &format!("/guilds/{guild}/threads/active"),
    )?))
}

pub fn archived_threads(token: &str, channel: &str) -> Result<Vec<Channel>, String> {
    Ok(threads_of(get(
        token,
        &format!("/channels/{channel}/threads/archived/public?limit=25"),
    )?))
}

pub fn start_thread(
    token: &str,
    channel: &str,
    message: &str,
    name: &str,
) -> Result<Channel, String> {
    send_json(
        "POST",
        token,
        &format!("/channels/{channel}/messages/{message}/threads"),
        json!({ "name": name, "auto_archive_duration": 1440 }),
    )?
    .into_json()
    .map_err(|e| e.to_string())
}

pub fn forum_post(token: &str, forum: &str, title: &str, content: &str) -> Result<Channel, String> {
    send_json(
        "POST",
        token,
        &format!("/channels/{forum}/threads"),
        json!({ "name": title, "auto_archive_duration": 1440, "message": { "content": content } }),
    )?
    .into_json()
    .map_err(|e| e.to_string())
}

/// Full profile, optionally as seen in a guild (nickname, roles, join date).
pub fn profile(token: &str, user_id: &str, guild: Option<&str>) -> Result<Profile, String> {
    let mut path =
        format!("/users/{user_id}/profile?with_mutual_guilds=true&with_mutual_friends_count=true");
    if let Some(g) = guild {
        path.push_str(&format!("&guild_id={g}"));
    }
    let v: Value = get(token, &path)?;
    let s = |x: &Value| x.as_str().filter(|t| !t.is_empty()).map(str::to_string);
    let mut user: User = serde_json::from_value(v["user"].clone()).unwrap_or_default();
    if user.id.is_empty() {
        user.id = user_id.to_string();
    }
    let meta = &v["user_profile"];
    let gmeta = &v["guild_member_profile"];
    // Guild-specific bio / pronouns win when set.
    let pick = |k: &str| {
        s(&gmeta[k])
            .or_else(|| s(&meta[k]))
            .or_else(|| s(&v["user"][k]))
    };
    Ok(Profile {
        bio: pick("bio"),
        pronouns: pick("pronouns"),
        accent_color: meta["accent_color"].as_u64().map(|c| c as u32),
        badges: v["badges"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|b| {
                Some(Badge {
                    description: b["description"].as_str()?.to_string(),
                    icon: b["icon"].as_str()?.to_string(),
                })
            })
            .collect(),
        connections: v["connected_accounts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| {
                Some(Connection {
                    kind: c["type"].as_str()?.to_string(),
                    name: c["name"].as_str()?.to_string(),
                    verified: c["verified"].as_bool().unwrap_or(false),
                })
            })
            .collect(),
        mutual_guilds: v["mutual_guilds"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|g| Some((g["id"].as_str()?.to_string(), s(&g["nick"]))))
            .collect(),
        mutual_friends_count: v["mutual_friends_count"].as_u64().unwrap_or(0) as u32,
        premium_since: s(&v["premium_since"]),
        premium_guild_since: s(&v["premium_guild_since"]),
        joined_at: s(&v["guild_member"]["joined_at"]),
        nick: s(&v["guild_member"]["nick"]),
        member_roles: v["guild_member"]["roles"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|r| r.as_str().map(str::to_string))
            .collect(),
        user,
    })
}

/// Recent messages that mention us (the "Inbox").
pub fn mentions(token: &str) -> Result<Vec<Message>, String> {
    get(
        token,
        "/users/@me/mentions?limit=25&roles=true&everyone=true",
    )
}

/// GIF categories ("Tendances", "Joyeux"…) shown before searching: (name, preview).
pub fn gif_categories(token: &str) -> Result<Vec<(String, String)>, String> {
    let v: Value = get(
        token,
        "/gifs/trending?provider=tenor&locale=fr&media_format=gif",
    )?;
    Ok(v["categories"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|c| {
            Some((
                c["name"].as_str()?.to_string(),
                c["src"].as_str()?.to_string(),
            ))
        })
        .collect())
}

/// A user settings protobuf (1 = preloaded, 2 = frecency / favourites).
pub fn settings_proto(token: &str, kind: u8) -> Result<Vec<u8>, String> {
    let v: Value = get(token, &format!("/users/@me/settings-proto/{kind}"))?;
    crate::proto::decode_b64(v["settings"].as_str().unwrap_or_default())
        .ok_or_else(|| "Paramètres illisibles".into())
}

pub fn set_settings_proto(token: &str, kind: u8, bytes: &[u8]) -> Result<(), String> {
    send_json(
        "PATCH",
        token,
        &format!("/users/@me/settings-proto/{kind}"),
        json!({ "settings": crate::proto::encode_b64(bytes) }),
    )?;
    Ok(())
}

pub fn guild_stickers(token: &str, guild: &str) -> Result<Vec<Sticker>, String> {
    get(token, &format!("/guilds/{guild}/stickers"))
}

pub fn send_sticker(token: &str, channel: &str, sticker: &str) -> Result<(), String> {
    send_json(
        "POST",
        token,
        &format!("/channels/{channel}/messages"),
        json!({ "sticker_ids": [sticker], "nonce": nonce(), "content": "" }),
    )?;
    Ok(())
}
