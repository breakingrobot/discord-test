mod api;

use std::time::Duration;

use gpui::{
    div, prelude::*, px, rgb, size, App, Application, Bounds, Context, FocusHandle, Focusable,
    KeyDownEvent, SharedString, Window, WindowBounds, WindowOptions,
};

use api::{Channel, Guild, Message};

const POLL_INTERVAL: Duration = Duration::from_secs(3);

struct DiscordApp {
    focus: FocusHandle,
    /// Bot token. Empty until login succeeds.
    token: String,
    logged_in: bool,
    input: String,
    status: String,
    guilds: Vec<Guild>,
    channels: Vec<Channel>,
    messages: Vec<Message>,
    guild: Option<String>,
    channel: Option<Channel>,
}

impl DiscordApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let mut this = Self {
            focus: cx.focus_handle(),
            token: String::new(),
            logged_in: false,
            input: std::env::var("DISCORD_TOKEN").unwrap_or_default(),
            status: "Paste a bot token and press Enter (or set DISCORD_TOKEN).".into(),
            guilds: vec![],
            channels: vec![],
            messages: vec![],
            guild: None,
            channel: None,
        };
        if !this.input.is_empty() {
            this.submit(cx);
        }
        this
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self.input.trim().to_string();
        if text.is_empty() {
            return;
        }
        if !self.logged_in {
            self.login(text, cx);
        } else if let Some(channel) = self.channel.clone() {
            self.input.clear();
            let token = self.token.clone();
            cx.spawn(async move |this, cx| {
                let res = cx
                    .background_spawn(async move { api::send(&token, &channel.id, &text) })
                    .await;
                this.update(cx, |this, cx| {
                    if let Err(e) = res {
                        this.status = format!("Send failed: {e}");
                    }
                    this.refresh_messages(cx);
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    fn login(&mut self, token: String, cx: &mut Context<Self>) {
        self.status = "Logging in…".into();
        cx.spawn(async move |this, cx| {
            let t = token.clone();
            let res = cx.background_spawn(async move { api::guilds(&t) }).await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(guilds) => {
                        this.token = token;
                        this.logged_in = true;
                        this.guilds = guilds;
                        this.input.clear();
                        this.status = "Pick a server.".into();
                        this.start_polling(cx);
                    }
                    Err(e) => this.status = format!("Login failed: {e}"),
                }
                cx.notify();
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
        let token = self.token.clone();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::channels(&token, &id) })
                .await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(c) => this.channels = c,
                    Err(e) => this.status = format!("Channels failed: {e}"),
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
                        this.messages = m;
                        this.status.clear();
                    }
                    Err(e) => this.status = format!("Messages failed: {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Simple polling instead of the gateway websocket, to keep this minimal.
    fn start_polling(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(POLL_INTERVAL).await;
            if this.update(cx, |this, cx| this.refresh_messages(cx)).is_err() {
                break;
            }
        })
        .detach();
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        match ks.key.as_str() {
            "enter" => self.submit(cx),
            "backspace" => {
                self.input.pop();
            }
            _ => {
                if ks.modifiers.control || ks.modifiers.platform {
                    return;
                }
                if let Some(ch) = &ks.key_char {
                    self.input.push_str(ch);
                }
            }
        }
        cx.notify();
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

        let shown_input: SharedString = if self.logged_in {
            self.input.clone().into()
        } else {
            "•".repeat(self.input.chars().count()).into()
        };
        let placeholder = if !self.logged_in {
            "Bot token"
        } else if self.channel.is_some() {
            "Message"
        } else {
            ""
        };

        let guild_list = div()
            .id("guilds")
            .w(px(180.))
            .h_full()
            .bg(rgb(0x1e1f22))
            .overflow_y_scroll()
            .p_2()
            .gap_1()
            .flex()
            .flex_col()
            .children(self.guilds.iter().map(|g| {
                let id = g.id.clone();
                let active = self.guild.as_deref() == Some(&g.id);
                div()
                    .id(SharedString::from(format!("g-{}", g.id)))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(active, |d| d.bg(rgb(0x404249)))
                    .hover(|d| d.bg(rgb(0x35373c)))
                    .child(g.name.clone())
                    .on_click(cx.listener(move |this, _, _, cx| this.select_guild(id.clone(), cx)))
            }));

        let channel_list = div()
            .id("channels")
            .w(px(200.))
            .h_full()
            .bg(rgb(0x2b2d31))
            .overflow_y_scroll()
            .p_2()
            .gap_1()
            .flex()
            .flex_col()
            .children(self.channels.iter().map(|c| {
                let chan = c.clone();
                let active = self.channel.as_ref().map(|s| &s.id) == Some(&c.id);
                div()
                    .id(SharedString::from(format!("c-{}", c.id)))
                    .px_2()
                    .py_1()
                    .rounded_md()
                    .cursor_pointer()
                    .when(active, |d| d.bg(rgb(0x404249)))
                    .hover(|d| d.bg(rgb(0x35373c)))
                    .child(format!("# {}", c.name.clone().unwrap_or_default()))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_channel(chan.clone(), cx)),
                    )
            }));

        let messages = div()
            .id("messages")
            .flex_1()
            .overflow_y_scroll()
            .p_3()
            .gap_2()
            .flex()
            .flex_col()
            .children(self.messages.iter().map(|m| {
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_color(rgb(0xf2f3f5))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(m.author.username.clone()),
                    )
                    .child(div().text_color(rgb(0xdbdee1)).child(m.content.clone()))
            }));

        let input_box = div()
            .m_3()
            .px_3()
            .py_2()
            .rounded_md()
            .bg(rgb(0x383a40))
            .child(if self.input.is_empty() {
                div().text_color(rgb(0x949ba4)).child(placeholder)
            } else {
                div().child(shown_input)
            });

        let main = div()
            .flex_1()
            .h_full()
            .flex()
            .flex_col()
            .bg(rgb(0x313338))
            .child(messages)
            .when(!self.status.is_empty(), |d| {
                d.child(
                    div()
                        .px_3()
                        .text_sm()
                        .text_color(rgb(0xf0b232))
                        .child(self.status.clone()),
                )
            })
            .when(!self.logged_in || self.channel.is_some(), |d| d.child(input_box));

        div()
            .id("root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .size_full()
            .flex()
            .text_color(rgb(0xdbdee1))
            .when(self.logged_in, |d| d.child(guild_list).child(channel_list))
            .child(main)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1000.), px(700.)), cx);
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
