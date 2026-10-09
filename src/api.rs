//! Blocking Discord REST helpers. Call these from a background executor.

use serde::Deserialize;

const BASE: &str = "https://discord.com/api/v10";

#[derive(Clone, Debug, Default, Deserialize)]
pub struct User {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub global_name: Option<String>,
}

impl User {
    pub fn display_name(&self) -> &str {
        self.global_name.as_deref().unwrap_or(&self.username)
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct Guild {
    pub id: String,
    pub name: String,
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
}

#[derive(Clone, Debug, Deserialize)]
pub struct Message {
    pub id: String,
    pub content: String,
    pub author: User,
    /// RFC 3339.
    pub timestamp: String,
    #[serde(default)]
    pub edited_timestamp: Option<String>,
    #[serde(default)]
    pub attachments: Vec<Attachment>,
    #[serde(default)]
    pub referenced_message: Option<Box<Message>>,
}

fn get<T: for<'de> Deserialize<'de>>(token: &str, path: &str) -> Result<T, String> {
    ureq::get(&format!("{BASE}{path}"))
        .set("Authorization", token)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())
}

pub fn me(token: &str) -> Result<User, String> {
    get(token, "/users/@me")
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

/// Oldest first.
pub fn messages(token: &str, channel: &str) -> Result<Vec<Message>, String> {
    let mut msgs: Vec<Message> = get(token, &format!("/channels/{channel}/messages?limit=50"))?;
    msgs.reverse();
    Ok(msgs)
}

pub fn send(token: &str, channel: &str, content: &str) -> Result<(), String> {
    ureq::post(&format!("{BASE}/channels/{channel}/messages"))
        .set("Authorization", token)
        .send_json(serde_json::json!({ "content": content }))
        .map_err(|e| e.to_string())?;
    Ok(())
}
