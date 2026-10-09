//! Blocking Discord REST helpers. Call these from a background executor.

use std::io::Read;

use serde::Deserialize;

const BASE: &str = "https://discord.com/api/v10";

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

fn auth_get(token: &str, path: &str) -> ureq::Request {
    ureq::get(&format!("{BASE}{path}")).set("Authorization", token)
}

fn get<T: for<'de> Deserialize<'de>>(token: &str, path: &str) -> Result<T, String> {
    auth_get(token, path)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())
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

pub fn send(
    token: &str,
    channel: &str,
    content: &str,
    reply_to: Option<&str>,
) -> Result<(), String> {
    let mut body = serde_json::json!({ "content": content });
    if let Some(id) = reply_to {
        body["message_reference"] = serde_json::json!({ "message_id": id });
    }
    ureq::post(&format!("{BASE}/channels/{channel}/messages"))
        .set("Authorization", token)
        .send_json(body)
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn edit(token: &str, channel: &str, message: &str, content: &str) -> Result<(), String> {
    ureq::patch(&format!("{BASE}/channels/{channel}/messages/{message}"))
        .set("Authorization", token)
        .send_json(serde_json::json!({ "content": content }))
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn delete(token: &str, channel: &str, message: &str) -> Result<(), String> {
    ureq::delete(&format!("{BASE}/channels/{channel}/messages/{message}"))
        .set("Authorization", token)
        .call()
        .map_err(|e| e.to_string())?;
    Ok(())
}

pub fn typing(token: &str, channel: &str) -> Result<(), String> {
    ureq::post(&format!("{BASE}/channels/{channel}/typing"))
        .set("Authorization", token)
        .call()
        .map_err(|e| e.to_string())?;
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
    let url = format!(
        "{BASE}/channels/{channel}/messages/{message}/reactions/{}/@me",
        percent_encode(&emoji.api_key())
    );
    let req = if add {
        ureq::put(&url)
    } else {
        ureq::delete(&url)
    };
    req.set("Authorization", token)
        .set("Content-Length", "0")
        .call()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Downloads an image from the CDN (capped at 8 MiB).
pub fn fetch_image(url: &str) -> Result<Vec<u8>, String> {
    let resp = ureq::get(url).call().map_err(|e| e.to_string())?;
    let mut bytes = Vec::new();
    resp.into_reader()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    Ok(bytes)
}
