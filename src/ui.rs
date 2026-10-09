//! Discord-style layout: server rail, channel sidebar, chat pane, login screen.

use chrono::{DateTime, Datelike, Local, Timelike};
use std::ops::Range;
use std::sync::Arc;

use gpui::{
    div, img, linear_color_stop, linear_gradient, prelude::*, px, rgb, rgba, AnyElement, Context,
    Div, ExternalPaths, FontStyle, FontWeight, HighlightStyle, Image, InteractiveText,
    KeyDownEvent, ObjectFit, SharedString, Stateful, StrikethroughStyle, StyledImage, StyledText,
    UnderlineStyle,
};

use crate::api::{Channel, Message, User};
use crate::{DiscordApp, Field, LoginMode, Picker, QrState};

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

fn initials_avatar(user_id: &str, name: &str, size: f32) -> Div {
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

fn round_image(image: Arc<Image>, size: f32, radius: Option<f32>) -> AnyElement {
    let el = img(image)
        .size(px(size))
        .flex_shrink_0()
        .object_fit(ObjectFit::Cover);
    match radius {
        Some(r) => el.rounded(px(r)).into_any_element(),
        None => el.rounded_full().into_any_element(),
    }
}

/// Resolves `<@id>`, `<#id>`, `<:emoji:id>` style tokens into readable text.
fn resolve_tokens(src: &str, lookup: &dyn Fn(char, &str) -> Option<String>) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find('>') else {
            out.push_str(tail);
            return out;
        };
        let inner = &tail[1..end];
        let replaced = if let Some(id) = inner.strip_prefix("@&") {
            Some(lookup('&', id).unwrap_or_else(|| "@rôle".into()))
        } else if let Some(id) = inner.strip_prefix("@!").or_else(|| inner.strip_prefix('@')) {
            Some(lookup('@', id).unwrap_or_else(|| "@utilisateur".into()))
        } else if let Some(id) = inner.strip_prefix('#') {
            Some(lookup('#', id).unwrap_or_else(|| "#salon".into()))
        } else {
            let e = inner.strip_prefix("a:").or_else(|| inner.strip_prefix(':'));
            e.and_then(|e| e.split(':').next())
                .map(|n| format!(":{n}:"))
        };
        match replaced {
            Some(r) => out.push_str(&r),
            None => out.push_str(&tail[..=end]),
        }
        rest = &tail[end + 1..];
    }
    out.push_str(rest);
    out
}

/// Tiny Markdown subset: **bold**, *italic*, __underline__, ~~strike~~, `code`, ```blocks```, links.
/// With `keep_markers` the output equals `src` (live styling for the composer).
fn markdown(src: &str, keep_markers: bool) -> (String, Vec<(Range<usize>, HighlightStyle)>) {
    let bold = HighlightStyle {
        font_weight: Some(FontWeight::BOLD),
        ..Default::default()
    };
    let italic = HighlightStyle {
        font_style: Some(FontStyle::Italic),
        ..Default::default()
    };
    let code = HighlightStyle {
        background_color: Some(rgb(0x1e1f22).into()),
        ..Default::default()
    };
    let under = HighlightStyle {
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            color: None,
            wavy: false,
        }),
        ..Default::default()
    };
    let strike = HighlightStyle {
        strikethrough: Some(StrikethroughStyle {
            thickness: px(1.),
            color: None,
        }),
        ..Default::default()
    };
    let link = HighlightStyle {
        color: Some(rgb(color::LINK).into()),
        underline: Some(UnderlineStyle {
            thickness: px(1.),
            color: None,
            wavy: false,
        }),
        ..Default::default()
    };
    let markers: [(&str, HighlightStyle); 7] = [
        ("```", code),
        ("**", bold),
        ("__", under),
        ("~~", strike),
        ("`", code),
        ("*", italic),
        ("_", italic),
    ];

    let mut out = String::new();
    let mut hl: Vec<(Range<usize>, HighlightStyle)> = Vec::new();
    let mut rest = src;
    'outer: while !rest.is_empty() {
        for (m, style) in &markers {
            if !rest.starts_with(m) {
                continue;
            }
            if *m == "_" && out.chars().last().is_some_and(|c| c.is_alphanumeric()) {
                continue;
            }
            let body = &rest[m.len()..];
            if let Some(end) = body.find(m) {
                let inner = &body[..end];
                if end > 0 && !inner.starts_with(char::is_whitespace) {
                    let start = out.len();
                    if keep_markers {
                        out.push_str(m);
                    }
                    out.push_str(inner);
                    if keep_markers {
                        out.push_str(m);
                    }
                    hl.push((start..out.len(), *style));
                    rest = &body[end + m.len()..];
                    continue 'outer;
                }
            }
        }
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
            let start = out.len();
            out.push_str(&rest[..end]);
            hl.push((start..out.len(), link));
            rest = &rest[end..];
            continue;
        }
        let c = rest.chars().next().unwrap();
        out.push(c);
        rest = &rest[c.len_utf8()..];
    }
    (out, hl)
}

fn acronym(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(3)
        .collect()
}

/// Server-rail slot: the active/hover pill on the left plus the round button.
fn mention_pill(count: u32) -> Div {
    div()
        .min_w(px(18.))
        .h(px(18.))
        .px(px(5.))
        .rounded_full()
        .bg(rgb(color::RED))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(0xffffff))
        .child(count.to_string())
}

fn badge(count: u32) -> Div {
    div()
        .absolute()
        .right(px(10.))
        .bottom(px(-2.))
        .min_w(px(18.))
        .h(px(18.))
        .px(px(5.))
        .rounded_full()
        .bg(rgb(color::RED))
        .border_2()
        .border_color(rgb(color::RAIL))
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(rgb(0xffffff))
        .child(count.to_string())
}

/// Server-rail slot: left pill (active / unread) plus the button and mention badge.
fn rail_slot(button: Stateful<Div>, active: bool, unread: bool, mentions: u32) -> Div {
    let tall = active;
    div()
        .relative()
        .w_full()
        .flex()
        .justify_center()
        .child(
            div()
                .absolute()
                .left_0()
                .top(if tall { px(4.) } else { px(20.) })
                .w(px(4.))
                .h(px(if tall { 40. } else { 8. }))
                .rounded_md()
                .bg(rgb(color::BRIGHT))
                .when(!active && !unread, |d| d.opacity(0.)),
        )
        .child(button)
        .when(mentions > 0, |d| d.child(badge(mentions)))
}

fn rail_button(
    id: impl Into<SharedString>,
    active: bool,
    label: String,
    icon: Option<Arc<Image>>,
) -> Stateful<Div> {
    let base = div().id(id.into()).size(px(48.)).cursor_pointer();
    if let Some(icon) = icon {
        return base.child(round_image(icon, 48., Some(if active { 16. } else { 24. })));
    }
    base.flex()
        .items_center()
        .justify_center()
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
        let switcher = self.switcher.is_some().then(|| self.switcher_overlay(cx));
        div()
            .id("root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .relative()
            .size_full()
            .text_color(rgb(color::TEXT))
            .text_size(px(15.))
            .child(body)
            .children(switcher)
    }

    fn switcher_overlay(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let items = self.switcher_items();
        let sel = self.switcher_sel.min(items.len().saturating_sub(1));
        let query = self.switcher.clone().unwrap_or_default();
        div()
            .id("switcher-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(rgba(0x000000a0))
            .flex()
            .justify_center()
            .pt(px(110.))
            .on_click(cx.listener(|this, _, _, cx| {
                this.switcher = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("switcher")
                    .w(px(560.))
                    .h_full()
                    .max_h(px(420.))
                    .rounded(px(8.))
                    .bg(rgb(color::CHAT))
                    .shadow_lg()
                    .p_3()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .h(px(44.))
                            .px_3()
                            .rounded(px(4.))
                            .bg(rgb(color::RAIL))
                            .flex()
                            .items_center()
                            .text_size(px(18.))
                            .child(if query.is_empty() {
                                div().flex().items_center().child(Self::caret()).child(
                                    div()
                                        .text_color(rgb(color::MUTED))
                                        .child("Où voulez-vous aller ?"),
                                )
                            } else {
                                div()
                                    .flex()
                                    .items_center()
                                    .child(query)
                                    .child(Self::caret())
                            }),
                    )
                    .children(items.into_iter().enumerate().map(|(i, (label, target))| {
                        div()
                            .id(SharedString::from(format!("sw-{i}")))
                            .h(px(36.))
                            .px_3()
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .cursor_pointer()
                            .when(i == sel, |d| {
                                d.bg(rgb(color::ACTIVE)).text_color(rgb(color::BRIGHT))
                            })
                            .hover(|d| d.bg(rgb(color::HOVER)))
                            .child(label)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.go(target.clone(), cx)),
                            )
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(color::MUTED))
                            .child("↑↓ naviguer · Entrée ouvrir · Échap fermer · Ctrl+K"),
                    ),
            )
            .into_any_element()
    }

    // ---- text input ----------------------------------------------------

    // ---- text input ----------------------------------------------------

    fn avatar(&self, u: &User, size: f32) -> AnyElement {
        match u
            .avatar_url()
            .and_then(|url| self.images.get(&url).cloned())
        {
            Some(i) => round_image(i, size, None),
            None => initials_avatar(&u.id, u.display_name(), size).into_any_element(),
        }
    }

    /// The editable text with a caret. `mask` hides the text (token entry);
    /// otherwise Markdown is styled live and newlines start new rows.
    fn input_line(&self, placeholder: String, mask: bool) -> Div {
        if self.input.is_empty() {
            return div()
                .flex()
                .items_center()
                .child(Self::caret())
                .child(div().text_color(rgb(color::MUTED)).child(placeholder));
        }
        let text: String = if mask {
            self.input
                .chars()
                .map(|c| if c == '\n' { c } else { '•' })
                .collect()
        } else {
            self.input.clone()
        };
        let hl = if mask {
            vec![]
        } else {
            markdown(&text, true).1
        };
        let caret_byte = text
            .char_indices()
            .nth(self.cursor)
            .map(|(i, _)| i)
            .unwrap_or(text.len());

        let mut col = div().flex().flex_col();
        let mut offset = 0;
        let mut caret_placed = false;
        for line in text.split('\n') {
            let (start, end) = (offset, offset + line.len());
            offset = end + 1;
            if !caret_placed && caret_byte <= end {
                caret_placed = true;
                let c = caret_byte - start;
                col = col.child(
                    div()
                        .flex()
                        .items_center()
                        .child(Self::styled(&line[..c], slice_hl(&hl, start, start + c)))
                        .child(Self::caret())
                        .child(Self::styled(&line[c..], slice_hl(&hl, start + c, end))),
                );
            } else {
                col = col.child(
                    div()
                        .min_h(px(22.))
                        .child(Self::styled(line, slice_hl(&hl, start, end))),
                );
            }
        }
        col
    }

    fn styled(text: &str, hl: Vec<(Range<usize>, HighlightStyle)>) -> AnyElement {
        if text.is_empty() {
            return div().into_any_element();
        }
        StyledText::new(text.to_string())
            .with_highlights(hl)
            .into_any_element()
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
        let show_qr = self.login_mode != LoginMode::Mfa;
        let card = div()
            .w(px(if show_qr { 820. } else { 480. }))
            .p(px(32.))
            .rounded(px(8.))
            .bg(rgb(color::CHAT))
            .shadow_lg()
            .flex()
            .gap(px(48.))
            .child(self.login_form(cx))
            .when(show_qr, |d| d.child(self.qr_panel(cx)));
        div()
            .size_full()
            .bg(linear_gradient(
                135.,
                linear_color_stop(rgb(0x5865f2), 0.),
                linear_color_stop(rgb(0x1e1f5c), 1.),
            ))
            .flex()
            .items_center()
            .justify_center()
            .child(card)
    }

    fn text_field(
        &self,
        label: &'static str,
        field: Field,
        mask: bool,
        cx: &mut Context<Self>,
    ) -> Div {
        let active = self.field == field;
        let shown: AnyElement = if active {
            self.input_line(String::new(), mask).into_any_element()
        } else {
            let t = &self.form[field as usize];
            let t = if mask {
                "•".repeat(t.chars().count())
            } else {
                t.clone()
            };
            div().child(t).into_any_element()
        };
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(
                div()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(color::MUTED))
                    .child(label),
            )
            .child(
                div()
                    .id(SharedString::from(label))
                    .h(px(40.))
                    .px_3()
                    .rounded(px(3.))
                    .bg(rgb(color::RAIL))
                    .border_2()
                    .border_color(rgb(if active { color::BRAND } else { color::RAIL }))
                    .flex()
                    .items_center()
                    .overflow_hidden()
                    .cursor_text()
                    .child(shown)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.set_field(field);
                        cx.notify();
                    })),
            )
    }

    fn link_button(
        &self,
        id: &'static str,
        label: &'static str,
        cx: &mut Context<Self>,
        on: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .text_sm()
            .text_color(rgb(color::LINK))
            .cursor_pointer()
            .hover(|d| d.underline())
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| on(this, cx)))
    }

    fn login_form(&mut self, cx: &mut Context<Self>) -> Div {
        let (title, sub) = match self.login_mode {
            LoginMode::Credentials => (
                "Content de te revoir !",
                "On est trop heureux de te revoir !",
            ),
            LoginMode::Token => (
                "Connexion par token",
                "Collez votre token d'authentification Discord.",
            ),
            LoginMode::Mfa => (
                "Authentification à deux facteurs",
                "Entrez le code à 6 chiffres de votre application d'authentification.",
            ),
        };
        let mut form = div().flex_1().min_w_0().flex().flex_col().gap_4().child(
            div()
                .flex()
                .flex_col()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .text_size(px(24.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(color::BRIGHT))
                        .child(title),
                )
                .child(div().text_color(rgb(color::MUTED)).child(sub)),
        );
        form = match self.login_mode {
            LoginMode::Credentials => form
                .child(self.text_field("E-MAIL OU NUMÉRO DE TÉLÉPHONE", Field::Email, false, cx))
                .child(self.text_field("MOT DE PASSE", Field::Password, true, cx)),
            LoginMode::Token => form.child(self.text_field("TOKEN", Field::Token, true, cx)),
            LoginMode::Mfa => form.child(self.text_field("CODE", Field::Code, false, cx)),
        };
        if !self.status.is_empty() {
            let busy = self.login_busy;
            form = form.child(
                div()
                    .text_sm()
                    .text_color(rgb(if busy { color::MUTED } else { color::RED }))
                    .child(self.status.clone()),
            );
        }
        let label = match self.login_mode {
            LoginMode::Credentials | LoginMode::Token => "Se connecter",
            LoginMode::Mfa => "Valider",
        };
        form = form.child(
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
                .when(self.login_busy, |d| d.opacity(0.6))
                .child(if self.login_busy {
                    "Connexion…"
                } else {
                    label
                })
                .on_click(cx.listener(|this, _, _, cx| this.submit_login(cx))),
        );
        let footer = match self.login_mode {
            LoginMode::Credentials => {
                self.link_button("use-token", "Utiliser un token à la place", cx, |t, cx| {
                    t.set_mode(LoginMode::Token, cx)
                })
            }
            _ => self.link_button("back", "← Retour", cx, |t, cx| {
                t.set_mode(LoginMode::Credentials, cx)
            }),
        };
        form.child(footer).child(
            div()
                .text_xs()
                .text_color(rgb(color::MUTED))
                .child("Votre session est enregistrée dans le trousseau du système."),
        )
    }

    fn qr_panel(&mut self, cx: &mut Context<Self>) -> Div {
        const SIZE: f32 = 176.;
        let code: AnyElement = match &self.qr {
            QrState::Ready if !self.qr_cells.is_empty() => {
                let n = self.qr_cells.len();
                let cell = (SIZE / n as f32).floor().max(2.);
                div()
                    .p_3()
                    .rounded(px(8.))
                    .bg(rgb(0xffffff))
                    .flex()
                    .flex_col()
                    .children(self.qr_cells.iter().map(|row| {
                        div().flex().children(row.iter().map(|dark| {
                            div()
                                .size(px(cell))
                                .bg(rgb(if *dark { 0x000000 } else { 0xffffff }))
                        }))
                    }))
                    .into_any_element()
            }
            _ => div()
                .size(px(SIZE + 24.))
                .rounded(px(8.))
                .bg(rgb(color::SIDEBAR))
                .flex()
                .items_center()
                .justify_center()
                .px_4()
                .text_sm()
                .text_color(rgb(color::MUTED))
                .child(match &self.qr {
                    QrState::Failed(e) => e.clone(),
                    QrState::Scanned(_) => "Code scanné".to_string(),
                    _ => "Génération du code…".to_string(),
                })
                .into_any_element(),
        };
        let (title, text) = match &self.qr {
            QrState::Scanned(name) => (
                "Vérifiez votre téléphone".to_string(),
                format!("Connexion en tant que {name}. Confirmez sur l'application mobile."),
            ),
            _ => (
                "Se connecter avec un code QR".to_string(),
                "Scannez-le avec l'application mobile Discord pour vous connecter instantanément."
                    .to_string(),
            ),
        };
        let refresh = matches!(self.qr, QrState::Failed(_)).then(|| {
            div()
                .id("qr-refresh")
                .px_3()
                .py_1()
                .rounded(px(3.))
                .bg(rgb(color::BRAND))
                .cursor_pointer()
                .text_sm()
                .text_color(rgb(0xffffff))
                .child("Actualiser")
                .on_click(cx.listener(|this, _, _, cx| this.start_qr(cx)))
        });
        div()
            .w(px(240.))
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .gap_3()
            .text_center()
            .child(code)
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(color::BRIGHT))
                    .child(title),
            )
            .child(div().text_sm().text_color(rgb(color::MUTED)).child(text))
            .children(refresh)
    }

    // ---- server rail ---------------------------------------------------

    fn rail(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let home_active = self.guild.is_none();
        let dm_unread = self.unread.values().any(|u| u.guild.is_none());
        let dm_mentions: u32 = self
            .unread
            .values()
            .filter(|u| u.guild.is_none())
            .map(|u| u.mentions)
            .sum();
        let home = rail_slot(
            rail_button("home", home_active, "DM".into(), None)
                .on_click(cx.listener(|this, _, _, cx| this.open_home(cx))),
            home_active,
            dm_unread,
            dm_mentions,
        );
        let guilds = self.guilds.iter().map(|g| {
            let active = self.guild.as_deref() == Some(&g.id);
            let id = g.id.clone();
            let icon = g.icon_url().and_then(|u| self.images.get(&u).cloned());
            let mine = || {
                self.unread
                    .values()
                    .filter(|u| u.guild.as_deref() == Some(&g.id))
            };
            let (unread, mentions) = (mine().next().is_some(), mine().map(|u| u.mentions).sum());
            rail_slot(
                rail_button(format!("g-{}", g.id), active, acronym(&g.name), icon)
                    .on_click(cx.listener(move |this, _, _, cx| this.select_guild(id.clone(), cx))),
                active,
                unread,
                mentions,
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
            .child(self.user_panel(cx))
    }

    fn dm_rows(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let friends_active = self.channel.is_none();
        let mut rows: Vec<AnyElement> = vec![
            div()
                .id("friends-entry")
                .h(px(42.))
                .px_2()
                .rounded(px(4.))
                .flex()
                .items_center()
                .gap_3()
                .cursor_pointer()
                .text_color(rgb(if friends_active {
                    color::BRIGHT
                } else {
                    color::MUTED
                }))
                .when(friends_active, |d| d.bg(rgb(color::ACTIVE)))
                .hover(|d| d.bg(rgb(color::HOVER)).text_color(rgb(color::BRIGHT)))
                .child(div().w(px(32.)).flex().justify_center().child("👥"))
                .child("Amis")
                .on_click(cx.listener(|this, _, _, cx| this.open_home(cx)))
                .into_any_element(),
            div()
                .px_2()
                .pt_3()
                .pb_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(color::MUTED))
                .child("MESSAGES PRIVÉS")
                .into_any_element(),
        ];
        for c in &self.dms {
            let active = self.channel.as_ref().map(|s| &s.id) == Some(&c.id);
            let chan = c.clone();
            let title = c.title();
            let pic = match c.recipients.first() {
                Some(u) => self.avatar(u, 32.),
                None => initials_avatar(&c.id, &title, 32.).into_any_element(),
            };
            let unread = self.unread.get(&c.id).cloned();
            let bright = active || unread.is_some();
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
                    .text_color(rgb(if bright { color::BRIGHT } else { color::MUTED }))
                    .when(unread.is_some(), |d| d.font_weight(FontWeight::SEMIBOLD))
                    .when(active, |d| d.bg(rgb(color::ACTIVE)))
                    .hover(|d| d.bg(rgb(color::HOVER)).text_color(rgb(color::BRIGHT)))
                    .child(pic)
                    .child(div().flex_1().overflow_hidden().child(title))
                    .children(
                        unread
                            .filter(|u| u.mentions > 0)
                            .map(|u| mention_pill(u.mentions)),
                    )
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
        let unread = self.unread.get(&c.id).cloned();
        let bright = active || unread.is_some();
        div()
            .id(SharedString::from(format!("c-{}", c.id)))
            .h(px(34.))
            .px_2()
            .rounded(px(4.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .text_color(rgb(if bright { color::BRIGHT } else { color::MUTED }))
            .when(unread.is_some(), |d| d.font_weight(FontWeight::SEMIBOLD))
            .when(active, |d| d.bg(rgb(color::ACTIVE)))
            .hover(|d| d.bg(rgb(color::HOVER)).text_color(rgb(color::BRIGHT)))
            .child(div().text_size(px(20.)).child("#"))
            .child(div().flex_1().overflow_hidden().child(c.title()))
            .children(
                unread
                    .filter(|u| u.mentions > 0)
                    .map(|u| mention_pill(u.mentions)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select_channel(chan.clone(), cx)))
            .into_any_element()
    }

    fn user_panel(&self, cx: &mut Context<Self>) -> Div {
        let me = self.me.clone().unwrap_or_default();
        div()
            .h(px(52.))
            .flex_shrink_0()
            .px_2()
            .bg(rgb(color::PANEL))
            .flex()
            .items_center()
            .gap_2()
            .child(
                div().relative().child(self.avatar(&me, 32.)).child(
                    div()
                        .absolute()
                        .right(px(-2.))
                        .bottom(px(-2.))
                        .size(px(12.))
                        .rounded_full()
                        .bg(rgb(if self.connected {
                            color::GREEN
                        } else {
                            color::MUTED
                        }))
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
                            .child(me.display_name().to_string()),
                    )
                    .child(div().text_xs().text_color(rgb(color::MUTED)).child(
                        if self.connected {
                            "En ligne"
                        } else {
                            "Hors connexion"
                        },
                    )),
            )
            .child(
                div()
                    .id("logout")
                    .px_2()
                    .py_1()
                    .rounded(px(4.))
                    .text_xs()
                    .text_color(rgb(color::MUTED))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::HOVER)).text_color(rgb(color::BRIGHT)))
                    .child("Quitter")
                    .on_click(cx.listener(|this, _, _, cx| this.logout(cx))),
            )
    }

    // ---- chat pane -----------------------------------------------------

    fn chat(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (prefix, title) = match &self.channel {
            Some(c) if c.is_dm() => ("@", c.title()),
            Some(c) => ("#", c.title()),
            None if self.guild.is_none() => ("", "Amis".to_string()),
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
            })
            .child(div().flex_1())
            .when(self.channel.is_some(), |d| {
                d.child(
                    div()
                        .id("members-toggle")
                        .px_2()
                        .py_1()
                        .rounded(px(4.))
                        .text_sm()
                        .cursor_pointer()
                        .text_color(rgb(if self.show_members {
                            color::BRIGHT
                        } else {
                            color::MUTED
                        }))
                        .hover(|d| d.bg(rgb(color::HOVER)))
                        .child("Membres")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.show_members = !this.show_members;
                            cx.notify();
                        })),
                )
            });

        let body: AnyElement = if self.channel.is_some() {
            let older = (!self.history_done && self.messages.len() >= 50).then(|| {
                div()
                    .id("older")
                    .mx_4()
                    .mt_2()
                    .py_1()
                    .flex()
                    .justify_center()
                    .rounded(px(4.))
                    .text_sm()
                    .text_color(rgb(color::LINK))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::HOVER)))
                    .child(if self.loading_older {
                        "Chargement…"
                    } else {
                        "Charger les messages précédents"
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.load_older(cx)))
            });
            let rows = self.message_rows(cx);
            div()
                .id("messages")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.scroll)
                .flex()
                .flex_col()
                .children(older)
                .child(self.welcome(prefix, &title))
                .children(rows)
                .child(div().h(px(16.)).flex_shrink_0())
                .into_any_element()
        } else if self.guild.is_none() {
            self.friends_view(cx)
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(color::MUTED))
                .child("Choisissez un salon.")
                .into_any_element()
        };

        let placeholder = if prefix == "@" {
            format!("Envoyer un message à @{title}")
        } else {
            format!("Envoyer un message dans #{title}")
        };

        // Reply / edit banner attached on top of the composer.
        let banner: Option<AnyElement> = if let Some(r) = &self.replying {
            Some(self.banner(format!("Réponse à {}", r.author.display_name()), cx))
        } else if self.editing.is_some() {
            Some(self.banner("Modification du message · Échap pour annuler".into(), cx))
        } else {
            None
        };
        let has_banner = banner.is_some();

        let composer = self.channel.is_some().then(|| {
            div()
                .px_4()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .children(banner)
                .child(
                    div()
                        .min_h(px(44.))
                        .py(px(10.))
                        .px_4()
                        .when(has_banner, |d| d.rounded_b(px(8.)))
                        .when(!has_banner, |d| d.rounded(px(8.)))
                        .bg(rgb(color::INPUT))
                        .flex()
                        .items_start()
                        .gap_3()
                        .child(
                            div()
                                .id("upload")
                                .size(px(24.))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(rgb(color::MUTED))
                                .hover(|d| d.bg(rgb(color::BRIGHT)))
                                .cursor_pointer()
                                .text_color(rgb(color::INPUT))
                                .font_weight(FontWeight::BOLD)
                                .flex()
                                .items_center()
                                .justify_center()
                                .child("+")
                                .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .child(self.input_line(placeholder, false)),
                        )
                        .child(
                            div()
                                .id("emoji-btn")
                                .flex_shrink_0()
                                .cursor_pointer()
                                .opacity(0.8)
                                .hover(|d| d.opacity(1.))
                                .child("🙂")
                                .on_click(cx.listener(|this, _, _, cx| {
                                    this.picker = match this.picker {
                                        Some(_) => None,
                                        None => Some(Picker::Composer),
                                    };
                                    cx.notify();
                                })),
                        ),
                )
                .child(
                    div()
                        .h(px(24.))
                        .text_xs()
                        .text_color(rgb(color::BRIGHT))
                        .child(self.typing_text().unwrap_or_default()),
                )
        });

        let main = div()
            .flex_1()
            .min_w_0()
            .h_full()
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
            .children(composer);

        let picker = self.picker.is_some().then(|| self.emoji_picker(cx));
        div()
            .id("chat")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(rgb(color::CHAT))
            .flex()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.upload(paths.paths().to_vec(), cx)
            }))
            .child(main)
            .when(self.show_members && self.channel.is_some(), |d| {
                d.child(self.members_panel())
            })
            .children(picker)
    }

    fn emoji_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut body = div()
            .id("emoji-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_2();
        for (name, list) in crate::emoji::CATEGORIES {
            body = body
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(color::MUTED))
                        .child(name.to_uppercase()),
                )
                .child(div().flex().flex_wrap().children(list.iter().map(|e| {
                    let e = e.to_string();
                    div()
                        .id(SharedString::from(format!("e-{e}")))
                        .size(px(36.))
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(22.))
                        .cursor_pointer()
                        .hover(|d| d.bg(rgb(color::HOVER)))
                        .child(e.clone())
                        .on_click(cx.listener(move |this, _, _, cx| this.pick_emoji(&e, cx)))
                })));
        }
        div()
            .id("emoji-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|this, _, _, cx| {
                this.picker = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("emoji-picker")
                    .absolute()
                    .right(px(24.))
                    .bottom(px(84.))
                    .w(px(360.))
                    .h(px(340.))
                    .rounded(px(8.))
                    .bg(rgb(color::SIDEBAR))
                    .border_1()
                    .border_color(rgb(color::RAIL))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(body),
            )
            .into_any_element()
    }

    fn friends_view(&self, cx: &mut Context<Self>) -> AnyElement {
        if self.friends.is_empty() {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(color::MUTED))
                .child("Aucun ami à afficher pour le moment.")
                .into_any_element();
        }
        div()
            .id("friends")
            .flex_1()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .child(
                div()
                    .pb_2()
                    .text_xs()
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(color::MUTED))
                    .child(format!("AMIS — {}", self.friends.len())),
            )
            .children(self.friends.iter().map(|u| {
                let user = u.clone();
                div()
                    .id(SharedString::from(format!("f-{}", u.id)))
                    .h(px(60.))
                    .px_3()
                    .rounded(px(8.))
                    .border_t_1()
                    .border_color(rgb(color::DIVIDER))
                    .flex()
                    .items_center()
                    .gap_3()
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::HOVER)))
                    .child(self.avatar(u, 36.))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(color::BRIGHT))
                                    .child(u.display_name().to_string()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(color::MUTED))
                                    .child(u.username.clone()),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open_dm_with(&user, cx)))
            }))
            .into_any_element()
    }

    fn banner(&self, text: String, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px_4()
            .py(px(6.))
            .rounded_t(px(8.))
            .bg(rgb(color::SIDEBAR))
            .flex()
            .items_center()
            .text_sm()
            .text_color(rgb(color::MUTED))
            .child(div().flex_1().overflow_hidden().child(text))
            .child(
                div()
                    .id("cancel-compose")
                    .px_2()
                    .cursor_pointer()
                    .hover(|d| d.text_color(rgb(color::BRIGHT)))
                    .child("✕")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cancel_compose();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn members_panel(&self) -> Stateful<Div> {
        let mut people: Vec<User> = Vec::new();
        let mut label = "PARTICIPANTS RÉCENTS";
        if let Some(c) = &self.channel {
            if c.is_dm() {
                label = "MEMBRES";
                people.extend(c.recipients.iter().cloned());
                people.extend(self.me.iter().cloned());
            } else {
                for m in self.messages.iter().rev() {
                    if !people.iter().any(|u| u.id == m.author.id) {
                        people.push(m.author.clone());
                    }
                }
            }
        }
        div()
            .id("members")
            .w(px(240.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(color::SIDEBAR))
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(
                div()
                    .px_2()
                    .pt_3()
                    .pb_1()
                    .text_xs()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color::MUTED))
                    .child(format!("{label} — {}", people.len())),
            )
            .children(people.iter().map(|u| {
                div()
                    .h(px(42.))
                    .px_2()
                    .flex()
                    .items_center()
                    .gap_3()
                    .rounded(px(4.))
                    .hover(|d| d.bg(rgb(color::HOVER)))
                    .child(self.avatar(u, 32.))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_color(rgb(color::MUTED))
                            .child(u.display_name().to_string()),
                    )
            }))
    }

    fn welcome(&self, prefix: &str, title: &str) -> Div {
        // Only when the whole history fits in what we fetched.
        if !self.history_done && self.messages.len() >= 50 {
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

    fn message_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
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
            rows.push(self.message_row(m, time.as_ref(), grouped, cx));
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

    /// Message text with mentions resolved and Markdown applied.
    fn rich_text(&self, m: &Message, edited: bool) -> AnyElement {
        let lookup = |kind: char, id: &str| -> Option<String> {
            match kind {
                '@' => m
                    .mentions
                    .iter()
                    .find(|u| u.id == id)
                    .map(|u| u.display_name().to_string())
                    .or_else(|| self.known_users.get(id).cloned())
                    .map(|n| format!("@{n}")),
                '#' => self
                    .channels
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| format!("#{}", c.title())),
                _ => None,
            }
        };
        let resolved = resolve_tokens(&m.content, &lookup);
        let (mut text, mut hl) = markdown(&resolved, false);
        if edited {
            let start = text.len();
            text.push_str(" (modifié)");
            hl.push((
                start..text.len(),
                HighlightStyle {
                    color: Some(rgb(color::MUTED).into()),
                    ..Default::default()
                },
            ));
        }
        let links = link_ranges(&text);
        let ranges: Vec<Range<usize>> = links.iter().map(|(r, _)| r.clone()).collect();
        let urls: Vec<String> = links.into_iter().map(|(_, u)| u).collect();
        InteractiveText::new(
            SharedString::from(format!("t-{}", m.id)),
            StyledText::new(text).with_highlights(hl),
        )
        .on_click(ranges, move |ix, _, cx| cx.open_url(&urls[ix]))
        .into_any_element()
    }

    fn attachment(&self, a: &crate::api::Attachment) -> AnyElement {
        if a.is_image() {
            let (w, h) = (
                a.width.unwrap_or(400) as f32,
                a.height.unwrap_or(300) as f32,
            );
            let scale = (400. / w).min(300. / h).min(1.);
            let (w, h) = ((w * scale).max(32.), (h * scale).max(32.));
            return match self.images.get(&a.preview_url()) {
                Some(i) => img(i.clone())
                    .w(px(w))
                    .h(px(h))
                    .rounded(px(8.))
                    .object_fit(ObjectFit::Contain)
                    .into_any_element(),
                None => div()
                    .w(px(w))
                    .h(px(h))
                    .rounded(px(8.))
                    .bg(rgb(color::SIDEBAR))
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_sm()
                    .text_color(rgb(color::MUTED))
                    .child("Chargement de l'image…")
                    .into_any_element(),
            };
        }
        div()
            .px_3()
            .py_2()
            .rounded(px(4.))
            .bg(rgb(color::SIDEBAR))
            .text_color(rgb(color::LINK))
            .child(a.filename.clone())
            .into_any_element()
    }

    fn embed(&self, e: &crate::api::Embed) -> AnyElement {
        let mut body = div().flex().flex_col().gap_1().min_w_0();
        if let Some(a) = &e.author {
            body = body.child(
                div()
                    .text_sm()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(a.name.clone()),
            );
        }
        if let Some(t) = &e.title {
            body = body.child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(if e.url.is_some() {
                        color::LINK
                    } else {
                        color::BRIGHT
                    }))
                    .child(t.clone()),
            );
        }
        if let Some(d) = &e.description {
            let (text, hl) = markdown(d, false);
            body = body.child(
                div()
                    .text_sm()
                    .child(StyledText::new(text).with_highlights(hl)),
            );
        }
        for f in &e.fields {
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color::BRIGHT))
                            .child(f.name.clone()),
                    )
                    .child(div().text_sm().child(f.value.clone())),
            );
        }
        if let Some(i) = e
            .image
            .as_ref()
            .and_then(|m| self.images.get(&m.preview_url()))
        {
            body = body.child(
                img(i.clone())
                    .w(px(360.))
                    .h(px(220.))
                    .rounded(px(4.))
                    .object_fit(ObjectFit::Contain),
            );
        }
        let mut row = div().flex().gap_3().child(body.flex_1());
        if let Some(i) = e
            .thumbnail
            .as_ref()
            .and_then(|m| self.images.get(&m.preview_url()))
        {
            row = row.child(
                img(i.clone())
                    .size(px(64.))
                    .flex_shrink_0()
                    .rounded(px(4.))
                    .object_fit(ObjectFit::Cover),
            );
        }
        div()
            .mt_1()
            .max_w(px(520.))
            .p_3()
            .rounded_r(px(4.))
            .bg(rgb(color::SIDEBAR))
            .border_l_4()
            .border_color(rgb(e.color.unwrap_or(color::DIVIDER)))
            .child(row)
            .into_any_element()
    }

    fn reactions(&self, m: &Message, cx: &mut Context<Self>) -> AnyElement {
        div()
            .mt_1()
            .flex()
            .flex_wrap()
            .gap_1()
            .children(m.reactions.iter().map(|r| {
                let (msg, emoji, me) = (m.clone(), r.emoji.clone(), r.me);
                let glyph: AnyElement =
                    match r.emoji.url().and_then(|u| self.images.get(&u).cloned()) {
                        Some(i) => img(i).size(px(18.)).into_any_element(),
                        None if r.emoji.id.is_some() => div()
                            .text_xs()
                            .child(format!(":{}:", r.emoji.label()))
                            .into_any_element(),
                        None => div().child(r.emoji.label()).into_any_element(),
                    };
                div()
                    .id(SharedString::from(format!(
                        "r-{}-{}",
                        m.id,
                        r.emoji.api_key()
                    )))
                    .h(px(24.))
                    .px(px(6.))
                    .rounded(px(6.))
                    .flex()
                    .items_center()
                    .gap_1()
                    .cursor_pointer()
                    .border_1()
                    .border_color(rgb(if me { color::BRAND } else { color::SIDEBAR }))
                    .bg(rgb(if me { 0x3b405a } else { color::SIDEBAR }))
                    .hover(|d| d.border_color(rgb(color::DIVIDER)))
                    .child(glyph)
                    .child(div().text_sm().child(r.count.to_string()))
                    .on_click(
                        cx.listener(move |this, _, _, cx| {
                            this.toggle_reaction(&msg, &emoji, me, cx)
                        }),
                    )
            }))
            .into_any_element()
    }

    /// Hover toolbar: reply, and edit/delete on our own messages.
    fn toolbar(&self, m: &Message, group: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let mine = self.me.as_ref().is_some_and(|u| u.id == m.author.id);
        let armed = self.confirm_delete.as_deref() == Some(&m.id);
        let btn = |id: String, label: &str, danger: bool| {
            div()
                .id(SharedString::from(id))
                .px_2()
                .py_1()
                .text_xs()
                .cursor_pointer()
                .text_color(rgb(if danger { color::RED } else { color::MUTED }))
                .hover(|d| d.bg(rgb(color::HOVER)))
                .child(label.to_string())
        };
        let (reply, edit, del, react) = (m.clone(), m.clone(), m.clone(), m.clone());
        let copy = m.content.clone();
        div()
            .absolute()
            .top(px(-14.))
            .right(px(16.))
            .flex()
            .rounded(px(4.))
            .border_1()
            .border_color(rgb(color::RAIL))
            .bg(rgb(color::CHAT))
            .opacity(0.)
            .group_hover(group, |s| s.opacity(1.))
            .child(
                btn(format!("react-{}", m.id), "Réagir", false).on_click(cx.listener(
                    move |this, _, _, cx| {
                        this.picker = Some(Picker::React(react.clone()));
                        cx.notify();
                    },
                )),
            )
            .child(
                btn(format!("reply-{}", m.id), "Répondre", false)
                    .on_click(cx.listener(move |this, _, _, cx| this.start_reply(&reply, cx))),
            )
            .child(
                btn(format!("copy-{}", m.id), "Copier", false).on_click(cx.listener(
                    move |_, _, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()))
                    },
                )),
            )
            .when(mine, |d| {
                d.child(
                    btn(format!("edit-{}", m.id), "Modifier", false)
                        .on_click(cx.listener(move |this, _, _, cx| this.start_edit(&edit, cx))),
                )
                .child(
                    btn(
                        format!("del-{}", m.id),
                        if armed { "Confirmer ?" } else { "Supprimer" },
                        true,
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.delete_message(&del, cx))),
                )
            })
            .into_any_element()
    }

    fn message_row(
        &self,
        m: &Message,
        time: Option<&DateTime<Local>>,
        grouped: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
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
            text = text.child(self.rich_text(m, m.edited_timestamp.is_some()));
        }
        for a in &m.attachments {
            text = text.child(div().mt_1().child(self.attachment(a)));
        }
        for e in &m.embeds {
            text = text.child(self.embed(e));
        }
        if !m.reactions.is_empty() {
            text = text.child(self.reactions(m, cx));
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
                .into_any_element()
        } else {
            self.avatar(&m.author, 40.)
        };

        let pinged = m.mention_everyone
            || self
                .me
                .as_ref()
                .is_some_and(|me| m.mentions.iter().any(|u| u.id == me.id));
        let mut row = div()
            .group(group.clone())
            .relative()
            .when(pinged, |d| {
                d.bg(rgb(0x444037)).border_l_2().border_color(rgb(0xf0b232))
            })
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
                    .child(self.avatar(&r.author, 16.))
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
            .child(self.toolbar(m, group, cx))
            .into_any_element()
    }
}

/// `http(s)://` runs in already-rendered text.
fn link_ranges(text: &str) -> Vec<(Range<usize>, String)> {
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find("http") {
        let start = from + i;
        let rest = &text[start..];
        if rest.starts_with("http://") || rest.starts_with("https://") {
            let end = start + rest.find(char::is_whitespace).unwrap_or(rest.len());
            out.push((start..end, text[start..end].to_string()));
            from = end;
        } else {
            from = start + 4;
        }
    }
    out
}

/// Highlights clipped to `from..to`, re-based to start at 0.
fn slice_hl(
    hl: &[(Range<usize>, HighlightStyle)],
    from: usize,
    to: usize,
) -> Vec<(Range<usize>, HighlightStyle)> {
    hl.iter()
        .filter_map(|(r, s)| {
            let (a, b) = (r.start.max(from), r.end.min(to));
            (a < b).then(|| (a - from..b - from, *s))
        })
        .collect()
}
