//! Blocking Discord REST helpers. Call these from a background executor.

use serde::Deserialize;

const BASE: &str = "https://discord.com/api/v10";

#[derive(Clone, Debug, Deserialize)]
pub struct Guild {
    pub id: String,
    pub name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Channel {
    pub id: String,
    pub name: Option<String>,
    #[serde(rename = "type")]
    pub kind: u8,
    #[serde(default)]
    pub position: i32,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Author {
    pub username: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct Message {
    pub content: String,
    pub author: Author,
}

fn get<T: for<'de> Deserialize<'de>>(token: &str, path: &str) -> Result<T, String> {
    ureq::get(&format!("{BASE}{path}"))
        .set("Authorization", token)
        .call()
        .map_err(|e| e.to_string())?
        .into_json()
        .map_err(|e| e.to_string())
}

pub fn guilds(token: &str) -> Result<Vec<Guild>, String> {
    get(token, "/users/@me/guilds")
}

/// Text channels only (type 0), ordered by position.
pub fn channels(token: &str, guild: &str) -> Result<Vec<Channel>, String> {
    let mut chans: Vec<Channel> = get(token, &format!("/guilds/{guild}/channels"))?;
    chans.retain(|c| c.kind == 0);
    chans.sort_by_key(|c| c.position);
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
