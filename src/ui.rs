//! Discord-style layout: server rail, channel sidebar, chat pane, login screen.

use chrono::{DateTime, Datelike, Local, Timelike};
use gpui::{
    div, prelude::*, px, rgb, AnyElement, Context, Div, FontWeight, KeyDownEvent, SharedString,
    Stateful,
};

use crate::api::{Channel, Message};
use crate::DiscordApp;

mod color {
    pub const RAIL: u32 = 0x1e1f22;
    pub const SIDEBAR: u32 = 0x2b2d31;
    pub const PANEL: u32 = 0x232428;
    pub const CHAT: u32 = 0x313338;
    pub const INPUT: u32 = 0x383a40;
    pub const HOVER: u32 = 0x35373c;
    pub const ACTIVE: u32 = 0x404249;
    pub const BRAND: u32 = 0x5865f2;
    pub const GREEN: u32 = 0x23a55a;
    pub const RED: u32 = 0xf23f42;
    pub const TEXT: u32 = 0xdbdee1;
    pub const BRIGHT: u32 = 0xf2f3f5;
    pub const MUTED: u32 = 0x949ba4;
    pub const LINK: u32 = 0x00a8fc;
    pub const DIVIDER: u32 = 0x3f4147;
}

const AVATARS: [u32; 6] = [0x5865f2, 0x747f8d, 0x3ba55c, 0xfaa81a, 0xed4245, 0xeb459e];
const MONTHS: [&str; 12] = [
    "janvier",
    "février",
    "mars",
    "avril",
    "mai",
    "juin",
    "juillet",
    "août",
    "septembre",
    "octobre",
    "novembre",
    "décembre",
];
/// Messages from the same author within this many minutes are visually grouped.
const GROUP_MINUTES: i64 = 7;

fn parse_time(ts: &str) -> Option<DateTime<Local>> {
    DateTime::parse_from_rfc3339(ts)
        .ok()
        .map(|d| d.with_timezone(&Local))
}

fn hhmm(t: &DateTime<Local>) -> String {
    format!("{:02}:{:02}", t.hour(), t.minute())
}

fn stamp(t: &DateTime<Local>) -> String {
    let today = Local::now().date_naive();
    let day = t.date_naive();
    if day == today {
        format!("Aujourd'hui à {}", hhmm(t))
    } else if day.succ_opt() == Some(today) {
        format!("Hier à {}", hhmm(t))
    } else {
        format!("{:02}/{:02}/{} {}", t.day(), t.month(), t.year(), hhmm(t))
    }
}

fn avatar(user_id: &str, name: &str, size: f32) -> Div {
    let hash = user_id
        .bytes()
        .fold(0usize, |a, b| a.wrapping_mul(31).wrapping_add(b as usize));
    let initial = name
        .chars()
        .next()
        .unwrap_or('?')
        .to_uppercase()
        .to_string();
    div()
        .size(px(size))
        .flex_shrink_0()
        .rounded_full()
        .bg(rgb(AVATARS[hash % AVATARS.len()]))
        .flex()
        .items_center()
        .justify_center()
        .text_color(rgb(0xffffff))
        .font_weight(FontWeight::BOLD)
        .text_size(px(size * 0.42))
        .child(initial)
}

fn acronym(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(3)
        .collect()
}

/// Server-rail slot: the active/hover pill on the left plus the round button.
fn rail_slot(button: Stateful<Div>, active: bool) -> Div {
    div()
        .relative()
        .w_full()
        .flex()
        .justify_center()
        .child(
            div()
                .absolute()
                .left_0()
                .top(px(4.))
                .w(px(4.))
                .h(px(if active { 40. } else { 8. }))
                .rounded_md()
                .bg(rgb(color::BRIGHT))
                .when(!active, |d| d.opacity(0.)),
        )
        .child(button)
}

fn rail_button(id: impl Into<SharedString>, active: bool, label: String) -> Stateful<Div> {
    div()
        .id(id.into())
        .size(px(48.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(rgb(color::BRIGHT))
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(15.))
        .child(label)
        .when(active, |d| d.rounded(px(16.)).bg(rgb(color::BRAND)))
        .when(!active, |d| {
            d.rounded_full()
                .bg(rgb(color::CHAT))
                .hover(|h| h.rounded(px(16.)).bg(rgb(color::BRAND)))
        })
}

impl DiscordApp {
    pub fn root(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let body = if self.logged_in {
            div()
                .size_full()
                .flex()
                .child(self.rail(cx))
                .child(self.sidebar(cx))
                .child(self.chat(cx))
        } else {
            self.login_view(cx)
        };
        div()
            .id("root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .size_full()
            .text_color(rgb(color::TEXT))
            .text_size(px(15.))
            .child(body)
    }

    // ---- text input ----------------------------------------------------

    /// The editable line with a caret. `mask` hides the text (token entry).
    fn input_line(&self, placeholder: String, mask: bool) -> Div {
        if self.input.is_empty() {
            return div()
                .flex()
                .items_center()
                .child(Self::caret())
                .child(div().text_color(rgb(color::MUTED)).child(placeholder));
        }
        let shown: Vec<char> = if mask {
            self.input.chars().map(|_| '•').collect()
        } else {
            self.input.chars().collect()
        };
        let at = self.cursor.min(shown.len());
        let before: String = shown[..at].iter().collect();
        let after: String = shown[at..].iter().collect();
        div()
            .flex()
            .items_center()
            .child(before)
            .child(Self::caret())
            .child(after)
    }

    fn caret() -> Div {
        div()
            .w(px(2.))
            .h(px(20.))
            .flex_shrink_0()
            .bg(rgb(color::BRIGHT))
    }

    // ---- login ---------------------------------------------------------

    fn login_view(&mut self, cx: &mut Context<Self>) -> Div {
        div()
            .size_full()
            .bg(rgb(color::BRAND))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(480.))
                    .p(px(32.))
                    .rounded(px(8.))
                    .bg(rgb(color::CHAT))
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .items_center()
                            .child(
                                div()
                                    .text_size(px(24.))
                                    .font_weight(FontWeight::BOLD)
                                    .text_color(rgb(color::BRIGHT))
                                    .child("Bon retour parmi nous !"),
                            )
                            .child(
                                div()
                                    .text_color(rgb(color::MUTED))
                                    .child("Collez votre token pour vous connecter."),
                            ),
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(color::MUTED))
                            .child("TOKEN"),
                    )
                    .child(
                        div()
                            .h(px(40.))
                            .px_2()
                            .rounded(px(4.))
                            .bg(rgb(color::RAIL))
                            .flex()
                            .items_center()
                            .overflow_hidden()
                            .child(self.input_line(String::new(), true)),
                    )
                    .when(!self.status.is_empty(), |d| {
                        let is_err = self.status != "Connexion…";
                        d.child(
                            div()
                                .text_sm()
                                .text_color(rgb(if is_err { color::RED } else { color::MUTED }))
                                .child(self.status.clone()),
                        )
                    })
                    .child(
                        div()
                            .id("login")
                            .h(px(44.))
                            .rounded(px(3.))
                            .bg(rgb(color::BRAND))
                            .hover(|h| h.bg(rgb(0x4752c4)))
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(0xffffff))
                            .font_weight(FontWeight::MEDIUM)
                            .child("Connexion")
                            .on_click(cx.listener(|this, _, _, cx| this.submit(cx))),
                    ),
            )
    }

    // ---- server rail ---------------------------------------------------

    fn rail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let home = rail_slot(
            rail_button("home", self.guild.is_none(), "DM".into())
                .on_click(cx.listener(|this, _, _, cx| this.open_home(cx))),
            self.guild.is_none(),
        );
        let guilds = self.guilds.iter().map(|g| {
            let active = self.guild.as_deref() == Some(&g.id);
            let id = g.id.clone();
            rail_slot(
                rail_button(format!("g-{}", g.id), active, acronym(&g.name))
                    .on_click(cx.listener(move |this, _, _, cx| this.select_guild(id.clone(), cx))),
                active,
            )
        });
        div()
            .id("rail")
            .w(px(72.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(color::RAIL))
            .py_3()
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .overflow_y_scroll()
            .child(home)
            .child(
                div()
                    .w(px(32.))
                    .h(px(2.))
                    .rounded_md()
                    .bg(rgb(color::DIVIDER)),
            )
            .children(guilds)
    }

    // ---- channel sidebar -----------------------------------------------

    fn sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let header_title = match &self.guild {
            None => "Messages privés".to_string(),
            Some(id) => self
                .guilds
                .iter()
                .find(|g| &g.id == id)
                .map(|g| g.name.clone())
                .unwrap_or_default(),
        };
        let header = div()
            .h(px(48.))
            .flex_shrink_0()
            .px_4()
            .flex()
            .items_center()
            .border_b_1()
            .border_color(rgb(color::RAIL))
            .text_color(rgb(color::BRIGHT))
            .font_weight(FontWeight::SEMIBOLD)
            .overflow_hidden()
            .child(header_title);

        let rows: Vec<AnyElement> = if self.guild.is_none() {
            self.dm_rows(cx)
        } else {
            self.channel_rows(cx)
        };

        div()
            .w(px(240.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(color::SIDEBAR))
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("channel-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .p_2()
                    .flex()
                    .flex_col()
                    .gap(px(2.))
                    .children(rows),
            )
            .child(self.user_panel())
    }

    fn dm_rows(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows: Vec<AnyElement> = vec![div()
            .px_2()
            .py_1()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(color::MUTED))
            .child("MESSAGES PRIVÉS")
            .into_any_element()];
        for c in &self.dms {
            let active = self.channel.as_ref().map(|s| &s.id) == Some(&c.id);
            let chan = c.clone();
            let title = c.title();
            let uid = c
                .recipients
                .first()
                .map(|r| r.id.clone())
                .unwrap_or_else(|| c.id.clone());
            rows.push(
                div()
                    .id(SharedString::from(format!("dm-{}", c.id)))
                    .h(px(42.))
                    .px_2()
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .gap_3()
                    .cursor_pointer()
                    .text_color(rgb(if active { color::BRIGHT } else { color::MUTED }))
                    .when(active, |d| d.bg(rgb(color::ACTIVE)))
                    .hover(|d| d.bg(rgb(color::HOVER)).text_color(rgb(color::BRIGHT)))
                    .child(avatar(&uid, &title, 32.))
                    .child(div().flex_1().overflow_hidden().child(title))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_channel(chan.clone(), cx)),
                    )
                    .into_any_element(),
            );
        }
        rows
    }

    fn channel_rows(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        let categories: Vec<&Channel> = self.channels.iter().filter(|c| c.kind == 4).collect();
        let loose = self
            .channels
            .iter()
            .filter(|c| c.kind != 4 && c.parent_id.is_none());
        for c in loose {
            rows.push(self.channel_row(c, cx));
        }
        for cat in categories {
            let collapsed = self.collapsed.contains(&cat.id);
            let cid = cat.id.clone();
            rows.push(
                div()
                    .id(SharedString::from(format!("cat-{}", cat.id)))
                    .mt_3()
                    .px_1()
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color::MUTED))
                    .hover(|d| d.text_color(rgb(color::BRIGHT)))
                    .child(if collapsed { "›" } else { "⌄" })
                    .child(cat.title().to_uppercase())
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.collapsed.remove(&cid) {
                            this.collapsed.insert(cid.clone());
                        }
                        cx.notify();
                    }))
                    .into_any_element(),
            );
            if collapsed {
                continue;
            }
            for c in self
                .channels
                .iter()
                .filter(|c| c.parent_id.as_deref() == Some(&cat.id))
            {
                rows.push(self.channel_row(c, cx));
            }
        }
        rows
    }

    fn channel_row(&self, c: &Channel, cx: &mut Context<Self>) -> AnyElement {
        let active = self.channel.as_ref().map(|s| &s.id) == Some(&c.id);
        let chan = c.clone();
        div()
            .id(SharedString::from(format!("c-{}", c.id)))
            .h(px(34.))
            .px_2()
            .rounded(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .text_color(rgb(if active { color::BRIGHT } else { color::MUTED }))
            .when(active, |d| d.bg(rgb(color::ACTIVE)))
            .hover(|d| d.bg(rgb(color::HOVER)).text_color(rgb(color::BRIGHT)))
            .child(div().text_size(px(20.)).child("#"))
            .child(div().flex_1().overflow_hidden().child(c.title()))
            .on_click(cx.listener(move |this, _, _, cx| this.select_channel(chan.clone(), cx)))
            .into_any_element()
    }

    fn user_panel(&self) -> Div {
        let (id, name, handle) = match &self.me {
            Some(u) => (
                u.id.clone(),
                u.display_name().to_string(),
                u.username.clone(),
            ),
            None => (String::new(), "?".into(), String::new()),
        };
        div()
            .h(px(52.))
            .flex_shrink_0()
            .px_2()
            .bg(rgb(color::PANEL))
            .flex()
            .items_center()
            .gap_2()
            .child(
                div().relative().child(avatar(&id, &name, 32.)).child(
                    div()
                        .absolute()
                        .right(px(-2.))
                        .bottom(px(-2.))
                        .size(px(12.))
                        .rounded_full()
                        .bg(rgb(color::GREEN))
                        .border_2()
                        .border_color(rgb(color::PANEL)),
                ),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color::BRIGHT))
                            .child(name),
                    )
                    .child(div().text_xs().text_color(rgb(color::MUTED)).child(handle)),
            )
    }

    // ---- chat pane -----------------------------------------------------

    fn chat(&mut self, _cx: &mut Context<Self>) -> impl IntoElement {
        let (prefix, title) = match &self.channel {
            Some(c) if c.is_dm() => ("@", c.title()),
            Some(c) => ("#", c.title()),
            None => ("", String::new()),
        };
        let topic = self
            .channel
            .as_ref()
            .and_then(|c| c.topic.clone())
            .unwrap_or_default();

        let header = div()
            .h(px(48.))
            .flex_shrink_0()
            .px_4()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(color::RAIL))
            .child(
                div()
                    .text_size(px(22.))
                    .text_color(rgb(color::MUTED))
                    .child(prefix),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color::BRIGHT))
                    .child(title.clone()),
            )
            .when(!topic.is_empty(), |d| {
                d.child(div().w(px(1.)).h(px(24.)).mx_1().bg(rgb(color::DIVIDER)))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_sm()
                            .text_color(rgb(color::MUTED))
                            .child(topic),
                    )
            });

        let body: AnyElement = if self.channel.is_some() {
            div()
                .id("messages")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .flex()
                .flex_col()
                .child(self.welcome(prefix, &title))
                .children(self.message_rows())
                .child(div().h(px(16.)).flex_shrink_0())
                .into_any_element()
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(color::MUTED))
                .child(if self.guild.is_none() {
                    "Choisissez une conversation."
                } else {
                    "Choisissez un salon."
                })
                .into_any_element()
        };

        let placeholder = if prefix == "@" {
            format!("Envoyer un message à @{title}")
        } else {
            format!("Envoyer un message dans #{title}")
        };

        div()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(rgb(color::CHAT))
            .flex()
            .flex_col()
            .child(header)
            .child(body)
            .when(!self.status.is_empty(), |d| {
                d.child(
                    div()
                        .px_4()
                        .pb_1()
                        .text_sm()
                        .text_color(rgb(color::RED))
                        .child(self.status.clone()),
                )
            })
            .when(self.channel.is_some(), |d| {
                d.child(
                    div().px_4().pb(px(24.)).flex_shrink_0().child(
                        div()
                            .min_h(px(44.))
                            .px_4()
                            .rounded(px(8.))
                            .bg(rgb(color::INPUT))
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(24.))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(rgb(color::MUTED))
                                    .text_color(rgb(color::INPUT))
                                    .font_weight(FontWeight::BOLD)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child("+"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .overflow_hidden()
                                    .child(self.input_line(placeholder, false)),
                            ),
                    ),
                )
            })
    }

    fn welcome(&self, prefix: &str, title: &str) -> Div {
        // Only when the whole history fits in what we fetched.
        if self.messages.len() >= 50 {
            return div();
        }
        div()
            .px_4()
            .pt(px(24.))
            .pb_2()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_size(px(28.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(color::BRIGHT))
                    .child(if prefix == "@" {
                        title.to_string()
                    } else {
                        format!("Bienvenue sur #{title} !")
                    }),
            )
            .child(div().text_color(rgb(color::MUTED)).child(if prefix == "@" {
                format!("Ceci est le début de votre historique de messages privés avec @{title}.")
            } else {
                format!("Ceci est le début du salon #{title}.")
            }))
    }

    fn message_rows(&self) -> Vec<AnyElement> {
        let mut rows = Vec::with_capacity(self.messages.len());
        let mut prev: Option<(&Message, DateTime<Local>)> = None;
        for m in &self.messages {
            let time = parse_time(&m.timestamp);
            let new_day = match (&prev, &time) {
                (Some((_, p)), Some(t)) => p.date_naive() != t.date_naive(),
                (None, Some(_)) => true,
                _ => false,
            };
            if new_day {
                if let Some(t) = &time {
                    rows.push(Self::day_divider(t));
                }
            }
            let grouped = !new_day
                && m.referenced_message.is_none()
                && match (&prev, &time) {
                    (Some((p, pt)), Some(t)) => {
                        p.author.id == m.author.id && (*t - *pt).num_minutes() < GROUP_MINUTES
                    }
                    _ => false,
                };
            rows.push(Self::message_row(m, time.as_ref(), grouped));
            if let Some(t) = time {
                prev = Some((m, t));
            }
        }
        rows
    }

    fn day_divider(t: &DateTime<Local>) -> AnyElement {
        let line = || div().flex_1().h(px(1.)).bg(rgb(color::DIVIDER));
        div()
            .mx_4()
            .my_2()
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(color::MUTED))
            .child(line())
            .child(format!(
                "{} {} {}",
                t.day(),
                MONTHS[t.month0() as usize],
                t.year()
            ))
            .child(line())
            .into_any_element()
    }

    fn message_row(m: &Message, time: Option<&DateTime<Local>>, grouped: bool) -> AnyElement {
        let group: SharedString = format!("m-{}", m.id).into();
        let name = m.author.display_name().to_string();

        let mut text = div().flex().flex_col().min_w_0().flex_1();
        if !grouped {
            let mut head = div().flex().items_baseline().gap_2().child(
                div()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(color::BRIGHT))
                    .child(name.clone()),
            );
            if let Some(t) = time {
                head = head.child(
                    div()
                        .text_xs()
                        .text_color(rgb(color::MUTED))
                        .child(stamp(t)),
                );
            }
            text = text.child(head);
        }
        if !m.content.is_empty() {
            text = text.child(div().child(m.content.clone()).when(
                m.edited_timestamp.is_some(),
                |d| {
                    d.child(
                        div()
                            .text_xs()
                            .text_color(rgb(color::MUTED))
                            .child(" (modifié)"),
                    )
                },
            ));
        }
        for a in &m.attachments {
            text = text.child(
                div()
                    .text_color(rgb(color::LINK))
                    .child(format!("📎 {}", a.filename)),
            );
        }

        let gutter = if grouped {
            div()
                .w(px(40.))
                .flex_shrink_0()
                .flex()
                .justify_center()
                .text_size(px(10.))
                .text_color(rgb(color::MUTED))
                .opacity(0.)
                .group_hover(group.clone(), |s| s.opacity(1.))
                .child(time.map(hhmm).unwrap_or_default())
        } else {
            avatar(&m.author.id, &name, 40.)
        };

        let mut row = div()
            .group(group)
            .px_4()
            .py(px(2.))
            .when(!grouped, |d| d.mt(px(14.)))
            .hover(|d| d.bg(rgb(0x2e3035)))
            .flex()
            .flex_col();

        if let Some(r) = &m.referenced_message {
            row = row.child(
                div()
                    .ml(px(20.))
                    .mb(px(2.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .text_color(rgb(color::MUTED))
                    .overflow_hidden()
                    .child("↱")
                    .child(
                        div()
                            .font_weight(FontWeight::MEDIUM)
                            .child(format!("@{}", r.author.display_name())),
                    )
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .child(r.content.replace('\n', " ")),
                    ),
            );
        }
        row.child(div().flex().gap_4().child(gutter).child(text))
            .into_any_element()
    }
}
