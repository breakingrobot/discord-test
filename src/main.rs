mod api;
mod ui;

use std::collections::HashSet;
use std::time::Duration;

use gpui::{
    prelude::*, px, size, App, Application, Bounds, ClipboardItem, Context, FocusHandle, Focusable,
    KeyDownEvent, Render, ScrollHandle, Window, WindowBounds, WindowOptions,
};

use api::{Channel, Guild, Message, User};

const POLL_INTERVAL: Duration = Duration::from_secs(3);

pub struct DiscordApp {
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// User or bot token. Empty until login succeeds.
    token: String,
    me: Option<User>,
    logged_in: bool,
    input: String,
    /// Caret position in `input`, counted in chars.
    cursor: usize,
    status: String,
    guilds: Vec<Guild>,
    dms: Vec<Channel>,
    channels: Vec<Channel>,
    messages: Vec<Message>,
    /// `None` is the Home / direct-messages view.
    guild: Option<String>,
    channel: Option<Channel>,
    collapsed: HashSet<String>,
    last_msg_id: Option<String>,
}

impl DiscordApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            token: String::new(),
            me: None,
            logged_in: false,
            input: String::new(),
            cursor: 0,
            status: String::new(),
            guilds: vec![],
            dms: vec![],
            channels: vec![],
            messages: vec![],
            guild: None,
            channel: None,
            collapsed: HashSet::new(),
            last_msg_id: None,
        };
        if let Ok(token) = std::env::var("DISCORD_TOKEN") {
            if !token.trim().is_empty() {
                this.login(token.trim().to_string(), cx);
            }
        }
        this
    }

    // ---- input editing -------------------------------------------------

    fn byte_idx(&self) -> usize {
        self.input
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(self.input.len())
    }

    fn insert(&mut self, s: &str) {
        let at = self.byte_idx();
        self.input.insert_str(at, s);
        self.cursor += s.chars().count();
    }

    fn clear_input(&mut self) {
        self.input.clear();
        self.cursor = 0;
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let len = self.input.chars().count();
        let cmd = ks.modifiers.control || ks.modifiers.platform;
        match ks.key.as_str() {
            "enter" => return self.submit(cx),
            "backspace" if self.cursor > 0 => {
                self.cursor -= 1;
                let at = self.byte_idx();
                self.input.remove(at);
            }
            "delete" if self.cursor < len => {
                let at = self.byte_idx();
                self.input.remove(at);
            }
            "left" => self.cursor = self.cursor.saturating_sub(1),
            "right" => self.cursor = (self.cursor + 1).min(len),
            "home" => self.cursor = 0,
            "end" => self.cursor = len,
            "v" if cmd => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.insert(&text.replace(['\r', '\n'], " "));
                }
            }
            "a" if cmd => self.cursor = len,
            "c" if cmd => cx.write_to_clipboard(ClipboardItem::new_string(self.input.clone())),
            _ if cmd => return,
            _ => match &ks.key_char {
                Some(ch) => self.insert(ch),
                None => return,
            },
        }
        cx.notify();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        if !self.logged_in {
            self.login(text, cx);
        } else if let Some(channel) = self.channel.clone() {
            self.clear_input();
            let token = self.token.clone();
            cx.spawn(async move |this, cx| {
                let res = cx
                    .background_spawn(async move { api::send(&token, &channel.id, &text) })
                    .await;
                this.update(cx, |this, cx| {
                    if let Err(e) = res {
                        this.status = format!("Échec de l'envoi : {e}");
                    }
                    // Always jump to our own message.
                    this.last_msg_id = None;
                    this.refresh_messages(cx);
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    // ---- data loading --------------------------------------------------

    fn login(&mut self, token: String, cx: &mut Context<Self>) {
        self.status = "Connexion…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let t = token.clone();
            let res = cx
                .background_spawn(async move {
                    Ok::<_, String>((
                        api::me(&t)?,
                        api::guilds(&t)?,
                        api::dms(&t).unwrap_or_default(),
                    ))
                })
                .await;
            this.update(cx, |this, cx| {
                match res {
                    Ok((me, guilds, dms)) => {
                        this.token = token;
                        this.me = Some(me);
                        this.guilds = guilds;
                        this.dms = dms;
                        this.logged_in = true;
                        this.status.clear();
                        this.clear_input();
                        this.start_polling(cx);
                    }
                    Err(e) => this.status = format!("Connexion impossible : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn open_home(&mut self, cx: &mut Context<Self>) {
        self.guild = None;
        self.channel = None;
        self.channels.clear();
        self.messages.clear();
        self.last_msg_id = None;
        self.refresh_dms(cx);
        cx.notify();
    }

    fn refresh_dms(&mut self, cx: &mut Context<Self>) {
        let token = self.token.clone();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { api::dms(&token) }).await;
            this.update(cx, |this, cx| {
                if let Ok(dms) = res {
                    this.dms = dms;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn select_guild(&mut self, id: String, cx: &mut Context<Self>) {
        self.guild = Some(id.clone());
        self.channels.clear();
        self.channel = None;
        self.messages.clear();
        self.last_msg_id = None;
        let token = self.token.clone();
        cx.spawn(async move |this, cx| {
            let gid = id.clone();
            let res = cx
                .background_spawn(async move { api::channels(&token, &gid) })
                .await;
            this.update(cx, |this, cx| {
                if this.guild.as_deref() != Some(&id) {
                    return;
                }
                match res {
                    Ok(c) => {
                        this.channels = c;
                        if let Some(first) = this.channels.iter().find(|c| c.kind != 4).cloned() {
                            this.select_channel(first, cx);
                        }
                    }
                    Err(e) => this.status = format!("Salons indisponibles : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn select_channel(&mut self, channel: Channel, cx: &mut Context<Self>) {
        self.channel = Some(channel);
        self.messages.clear();
        self.last_msg_id = None;
        self.status.clear();
        self.refresh_messages(cx);
        cx.notify();
    }

    fn refresh_messages(&mut self, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        let token = self.token.clone();
        let id = channel.id.clone();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::messages(&token, &id) })
                .await;
            this.update(cx, |this, cx| {
                // Ignore stale responses for a channel we've since left.
                if this.channel.as_ref().map(|c| &c.id) != Some(&channel.id) {
                    return;
                }
                match res {
                    Ok(m) => {
                        let newest = m.last().map(|x| x.id.clone());
                        if newest != this.last_msg_id {
                            // Follow new messages only if we were already at the bottom.
                            let off = this.scroll.offset().y;
                            let at_bottom = this.last_msg_id.is_none()
                                || off <= -this.scroll.max_offset().height + px(80.);
                            if at_bottom {
                                this.scroll.scroll_to_bottom();
                            }
                            this.last_msg_id = newest;
                        }
                        this.messages = m;
                        this.status.clear();
                    }
                    Err(e) => this.status = format!("Messages indisponibles : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Simple polling instead of the gateway websocket, to keep this lightweight.
    fn start_polling(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            let alive = this.update(cx, |this, cx| {
                this.refresh_messages(cx);
                if this.guild.is_none() {
                    this.refresh_dms(cx);
                }
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();
    }
}

impl Focusable for DiscordApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for DiscordApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        self.root(cx)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1200.), px(760.)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                ..Default::default()
            },
            |_, cx| cx.new(DiscordApp::new),
        )
        .unwrap();
        cx.activate(true);
    });
}
