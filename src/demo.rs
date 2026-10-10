//! Offline demo data (`DISCORD_DEMO=1`): lets the UI be previewed without an account.

use serde_json::{json, Value};

use crate::api::{Channel, Guild, Message, User};

fn user(id: &str, name: &str, global: &str) -> Value {
    json!({ "id": id, "username": name, "global_name": global })
}

fn parse<T: serde::de::DeserializeOwned>(v: Value) -> T {
    serde_json::from_value(v).expect("demo fixture")
}

fn ts(min_ago: i64) -> String {
    (chrono::Utc::now() - chrono::Duration::minutes(min_ago)).to_rfc3339()
}

pub struct Demo {
    pub me: User,
    pub guilds: Vec<Guild>,
    pub dms: Vec<Channel>,
    pub channels: Vec<Channel>,
    pub messages: Vec<Message>,
    pub friends: Vec<User>,
}

pub fn data() -> Demo {
    let me = user("100000000000000001", "toi", "Toi");
    let alice = user("100000000000000002", "alice", "Alice");
    let bob = user("100000000000000003", "bob_le_bricoleur", "Bob");
    let chloe = user("100000000000000004", "chloe", "Chloé");
    let bot = json!({ "id": "100000000000000005", "username": "Mee7", "bot": true });
    let g = "200000000000000001";
    let guilds = parse(json!([
        { "id": g, "name": "Rust Francophone" },
        { "id": "200000000000000002", "name": "GPUI Lab" },
        { "id": "200000000000000003", "name": "Gaming Night" },
        { "id": "200000000000000004", "name": "Design Club" },
    ]));
    let channels = parse(json!([
        { "id": "300000000000000001", "name": "accueil", "type": 4, "position": 0 },
        { "id": "300000000000000002", "name": "annonces", "type": 5, "position": 1, "parent_id": "300000000000000001" },
        { "id": "300000000000000003", "name": "général", "type": 0, "position": 2, "parent_id": "300000000000000001",
          "topic": "Discussions générales autour de Rust et de GPUI" },
        { "id": "300000000000000004", "name": "aide", "type": 0, "position": 3, "parent_id": "300000000000000001" },
        { "id": "300000000000000005", "name": "projets", "type": 4, "position": 4 },
        { "id": "300000000000000006", "name": "showcase", "type": 15, "position": 5, "parent_id": "300000000000000005" },
        { "id": "300000000000000007", "name": "client-discord", "type": 0, "position": 6, "parent_id": "300000000000000005" },
    ]));
    let ch = "300000000000000003";
    let msg = |id: &str, author: &Value, content: &str, min_ago: i64, extra: Value| {
        let mut m = json!({
            "id": id, "channel_id": ch, "guild_id": g, "author": author,
            "content": content, "timestamp": ts(min_ago),
        });
        if let (Some(o), Some(e)) = (m.as_object_mut(), extra.as_object()) {
            for (k, v) in e {
                o.insert(k.clone(), v.clone());
            }
        }
        m
    };
    let first = msg(
        "400000000000000001",
        &alice,
        "Salut tout le monde ! Quelqu'un a déjà essayé **GPUI** pour une app de bureau ?",
        180,
        json!({}),
    );
    let messages = parse(json!([
        msg("400000000000000000", &chloe, "", 200, json!({ "type": 7 })),
        first.clone(),
        msg("400000000000000002", &alice, "Je trouve le rendu *super* fluide, même avec beaucoup de texte.", 179, json!({})),
        msg("400000000000000003", &bob, "Oui ! Regarde https://github.com/zed-industries/zed — tout l'éditeur est fait avec.", 170,
            json!({ "type": 19, "referenced_message": first, "reactions": [
                { "count": 3, "me": true, "emoji": { "name": "🔥" } },
                { "count": 1, "me": false, "emoji": { "name": "👍" } } ] })),
        msg("400000000000000004", &me, "Je suis justement en train de faire un client Discord avec : `cargo run --release` 🚀", 60,
            json!({ "edited_timestamp": ts(58) })),
        msg("400000000000000005", &bot, "", 59, json!({ "embeds": [{
            "title": "Nouveau commit sur client-discord", "color": 5793266,
            "description": "**feat:** thème Onyx, icônes Lucide et police Inter",
            "fields": [{ "name": "Branche", "value": "main" }, { "name": "Auteur", "value": "toi" }] }] })),
        msg("400000000000000006", &chloe, "<@100000000000000001> c'est trop beau ! Tu peux partager une capture ? On en parle <t:1792000000:R>", 20,
            json!({ "mentions": [me] })),
        msg("400000000000000007", &alice, "~~Electron~~ natif, enfin 😄", 5, json!({})),
    ]));
    let dms = parse(json!([
        { "id": "500000000000000001", "type": 1, "recipients": [alice] },
        { "id": "500000000000000002", "type": 1, "recipients": [bob] },
        { "id": "500000000000000003", "type": 3, "name": "Projet secret", "recipients": [alice, chloe] },
    ]));
    let friends = parse(json!([alice, bob, chloe]));
    Demo {
        me: parse(me),
        guilds,
        dms,
        channels,
        messages,
        friends,
    }
}
