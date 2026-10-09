//! Discord-style layout: server rail, channel sidebar, chat pane, login screen.

use chrono::{DateTime, Datelike, Local, Timelike};
use std::ops::Range;
use std::sync::Arc;

use gpui::{
    div, img, linear_color_stop, linear_gradient, prelude::*, pulsating_between, px, rgb, rgba,
    Animation, AnimationExt, AnyElement, Context, Div, ExternalPaths, FontStyle, FontWeight,
    HighlightStyle, Image, InteractiveText, KeyDownEvent, ObjectFit, SharedString, Stateful,
    StrikethroughStyle, StyledImage, StyledText, UnderlineStyle,
};

use crate::api::{Channel, Message, User};
use crate::gateway::MemberRow;
use crate::{DiscordApp, Field, LoginMode, Picker, PickerTab, QrState, Side};

/// Theme palette: dark by default, light when `set_light(true)`.
pub mod color {
    use std::sync::atomic::{AtomicBool, Ordering};

    static LIGHT: AtomicBool = AtomicBool::new(false);

    pub fn set_light(on: bool) {
        LIGHT.store(on, Ordering::Relaxed);
    }

    pub fn is_light() -> bool {
        LIGHT.load(Ordering::Relaxed)
    }

    fn pick(dark: u32, light: u32) -> u32 {
        if is_light() {
            light
        } else {
            dark
        }
    }

    pub fn rail() -> u32 {
        pick(0x1e1f22, 0xe3e5e8)
    }

    pub fn sidebar() -> u32 {
        pick(0x2b2d31, 0xf2f3f5)
    }

    pub fn panel() -> u32 {
        pick(0x232428, 0xebedef)
    }

    pub fn chat() -> u32 {
        pick(0x313338, 0xffffff)
    }

    pub fn input() -> u32 {
        pick(0x383a40, 0xebedef)
    }

    pub fn hover() -> u32 {
        pick(0x35373c, 0xdfe1e5)
    }

    pub fn active() -> u32 {
        pick(0x404249, 0xd1d3d8)
    }

    pub fn brand() -> u32 {
        pick(0x5865f2, 0x5865f2)
    }

    pub fn green() -> u32 {
        pick(0x23a55a, 0x23a55a)
    }

    pub fn red() -> u32 {
        pick(0xf23f42, 0xd83c3e)
    }

    pub fn text() -> u32 {
        pick(0xdbdee1, 0x2e3338)
    }

    pub fn bright() -> u32 {
        pick(0xf2f3f5, 0x060607)
    }

    pub fn muted() -> u32 {
        pick(0x949ba4, 0x5c5e66)
    }

    pub fn link() -> u32 {
        pick(0x00a8fc, 0x006ce7)
    }

    pub fn divider() -> u32 {
        pick(0x3f4147, 0xd7d9dd)
    }

    pub fn msg_hover() -> u32 {
        pick(0x2e3035, 0xf7f7f8)
    }

    pub fn ping_bg() -> u32 {
        pick(0x444037, 0xfff6dc)
    }

    pub fn react_me() -> u32 {
        pick(0x3b405a, 0xe3e6fc)
    }

    pub fn code_bg() -> u32 {
        pick(0x1e1f22, 0xe3e5e8)
    }
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

/// Pulsing placeholder block shown while content loads.
fn skeleton(
    id: impl Into<SharedString>,
    w: Option<f32>,
    h: f32,
    radius: Option<f32>,
) -> AnyElement {
    let id: SharedString = id.into();
    let base = div().h(px(h)).flex_shrink_0().bg(rgb(color::active()));
    let base = match w {
        Some(w) => base.w(px(w)),
        None => base.w_full(),
    };
    let base = match radius {
        Some(r) => base.rounded(px(r)),
        None => base.rounded_full(),
    };
    base.with_animation(
        id,
        Animation::new(std::time::Duration::from_millis(1400))
            .repeat()
            .with_easing(pulsating_between(0.35, 0.9)),
        |d, v| d.opacity(v),
    )
    .into_any_element()
}

fn status_color(status: &str) -> Option<u32> {
    match status {
        "online" => Some(color::green()),
        "idle" => Some(0xf0b232),
        "dnd" => Some(color::red()),
        _ => None,
    }
}

fn status_label(status: &str) -> &'static str {
    match status {
        "online" => "En ligne",
        "idle" => "Absent",
        "dnd" => "Ne pas déranger",
        _ => "Hors ligne",
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
        background_color: Some(rgb(color::code_bg()).into()),
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
        color: Some(rgb(color::link()).into()),
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
        .bg(rgb(color::red()))
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
        .bg(rgb(color::red()))
        .border_2()
        .border_color(rgb(color::rail()))
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
                .bg(rgb(color::bright()))
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
        .text_color(rgb(color::bright()))
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(15.))
        .child(label)
        .when(active, |d| d.rounded(px(16.)).bg(rgb(color::brand())))
        .when(!active, |d| {
            d.rounded_full()
                .bg(rgb(color::chat()))
                .hover(|h| h.rounded(px(16.)).bg(rgb(color::brand())))
        })
}

impl DiscordApp {
    pub fn root(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = self.width < 760.;
        let body = if self.logged_in && narrow {
            // Phone-like layout: chat only, navigation slides over it.
            let nav = self.nav_open.then(|| {
                div()
                    .id("nav-backdrop")
                    .absolute()
                    .top_0()
                    .left_0()
                    .size_full()
                    .bg(rgba(0x00000080))
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.nav_open = false;
                        cx.notify();
                    }))
                    .child(
                        div()
                            .id("nav-panel")
                            .h_full()
                            .flex()
                            .shadow_lg()
                            .on_click(|_, _, cx| cx.stop_propagation())
                            .child(self.rail(cx))
                            .child(self.sidebar(cx)),
                    )
            });
            div()
                .size_full()
                .relative()
                .flex()
                .child(self.chat(cx))
                .children(nav)
        } else if self.logged_in {
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
        let profile = self.profile.clone().map(|u| self.profile_overlay(&u, cx));
        let settings = self.settings.then(|| self.settings_overlay(cx));
        div()
            .id("root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .relative()
            .size_full()
            .text_color(rgb(color::text()))
            .text_size(px(15.))
            .child(body)
            .children(profile)
            .children(settings)
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
                    .w(px(560.0_f32.min(self.width - 32.)))
                    .h_full()
                    .max_h(px(420.))
                    .rounded(px(8.))
                    .bg(rgb(color::chat()))
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
                            .bg(rgb(color::rail()))
                            .flex()
                            .items_center()
                            .text_size(px(18.))
                            .child(if query.is_empty() {
                                div().flex().items_center().child(Self::caret()).child(
                                    div()
                                        .text_color(rgb(color::muted()))
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
                                d.bg(rgb(color::active())).text_color(rgb(color::bright()))
                            })
                            .hover(|d| d.bg(rgb(color::hover())))
                            .child(label)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.go(target.clone(), cx)),
                            )
                    }))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(color::muted()))
                            .child("↑↓ naviguer · Entrée ouvrir · Échap fermer · Ctrl+K"),
                    ),
            )
            .into_any_element()
    }

    // ---- text input ----------------------------------------------------

    // ---- text input ----------------------------------------------------

    /// Avatar with a presence dot (when the user is online / idle / dnd).
    fn avatar_status(&self, u: &User, size: f32, ring: u32) -> AnyElement {
        let dot = self.presence.get(&u.id).and_then(|s| status_color(s));
        div()
            .relative()
            .flex_shrink_0()
            .child(self.avatar(u, size))
            .children(dot.map(|c| {
                div()
                    .absolute()
                    .right(px(-2.))
                    .bottom(px(-2.))
                    .size(px(size * 0.36))
                    .rounded_full()
                    .bg(rgb(c))
                    .border_2()
                    .border_color(rgb(ring))
            }))
            .into_any_element()
    }

    fn avatar(&self, u: &User, size: f32) -> AnyElement {
        let Some(url) = u.avatar_url() else {
            return initials_avatar(&u.id, u.display_name(), size).into_any_element();
        };
        match self.images.get(&url) {
            Some(i) => round_image(i.clone(), size, None),
            None if self.image_failed.contains(&url) => {
                initials_avatar(&u.id, u.display_name(), size).into_any_element()
            }
            None => skeleton(
                format!("sk-av-{}-{}", u.id, size as u32),
                Some(size),
                size,
                None,
            ),
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
                .child(div().text_color(rgb(color::muted())).child(placeholder));
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
            .bg(rgb(color::bright()))
    }

    // ---- login ---------------------------------------------------------

    fn login_view(&mut self, cx: &mut Context<Self>) -> Div {
        let show_qr = self.login_mode != LoginMode::Mfa && self.width >= 800.;
        let card = div()
            .w(px(
                (if show_qr { 820.0_f32 } else { 480.0 }).min(self.width - 32.)
            ))
            .p(px(32.))
            .rounded(px(8.))
            .bg(rgb(color::chat()))
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
                    .text_color(rgb(color::muted()))
                    .child(label),
            )
            .child(
                div()
                    .id(SharedString::from(label))
                    .h(px(40.))
                    .px_3()
                    .rounded(px(3.))
                    .bg(rgb(color::rail()))
                    .border_2()
                    .border_color(rgb(if active {
                        color::brand()
                    } else {
                        color::rail()
                    }))
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
            .text_color(rgb(color::link()))
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
                        .text_color(rgb(color::bright()))
                        .child(title),
                )
                .child(div().text_color(rgb(color::muted())).child(sub)),
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
                    .text_color(rgb(if busy { color::muted() } else { color::red() }))
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
                .bg(rgb(color::brand()))
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
                .text_color(rgb(color::muted()))
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
                .bg(rgb(color::sidebar()))
                .flex()
                .items_center()
                .justify_center()
                .px_4()
                .text_sm()
                .text_color(rgb(color::muted()))
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
                .bg(rgb(color::brand()))
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
                    .text_color(rgb(color::bright()))
                    .child(title),
            )
            .child(div().text_sm().text_color(rgb(color::muted())).child(text))
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
            .bg(rgb(color::rail()))
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
                    .bg(rgb(color::divider())),
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
            .border_color(rgb(color::rail()))
            .text_color(rgb(color::bright()))
            .font_weight(FontWeight::SEMIBOLD)
            .overflow_hidden()
            .child(header_title);

        let rows: Vec<AnyElement> = if self.guild.is_none() {
            self.dm_rows(cx)
        } else if self.channels.is_empty() {
            (0..9)
                .map(|i| {
                    div()
                        .px_2()
                        .py(px(6.))
                        .child(skeleton(
                            format!("sk-ch-{i}"),
                            Some([140., 100., 160., 120., 90., 150., 110., 130., 100.][i]),
                            14.,
                            Some(6.),
                        ))
                        .into_any_element()
                })
                .collect()
        } else {
            self.channel_rows(cx)
        };

        div()
            .w(px(if self.width < 900. { 200. } else { 240. }))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(color::sidebar()))
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
                    color::bright()
                } else {
                    color::muted()
                }))
                .when(friends_active, |d| d.bg(rgb(color::active())))
                .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
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
                .text_color(rgb(color::muted()))
                .child("MESSAGES PRIVÉS")
                .into_any_element(),
        ];
        for c in &self.dms {
            let active = self.channel.as_ref().map(|s| &s.id) == Some(&c.id);
            let chan = c.clone();
            let title = c.title();
            let pic = match c.recipients.first() {
                Some(u) if c.recipients.len() == 1 => self.avatar_status(u, 32., color::sidebar()),
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
                    .text_color(rgb(if bright {
                        color::bright()
                    } else {
                        color::muted()
                    }))
                    .when(unread.is_some(), |d| d.font_weight(FontWeight::SEMIBOLD))
                    .when(active, |d| d.bg(rgb(color::active())))
                    .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
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
            rows.extend(self.thread_rows(&c.id, cx));
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
                    .text_color(rgb(color::muted()))
                    .hover(|d| d.text_color(rgb(color::bright())))
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
                rows.extend(self.thread_rows(&c.id, cx));
            }
        }
        rows
    }

    /// Active threads under a channel, indented in the sidebar.
    fn thread_rows(&self, parent: &str, cx: &mut Context<Self>) -> Vec<AnyElement> {
        self.threads
            .iter()
            .filter(|t| t.parent_id.as_deref() == Some(parent))
            .filter(|_| !self.channels.iter().any(|c| c.id == parent && c.is_forum()))
            .map(|t| {
                let chan = t.clone();
                let active = self.channel.as_ref().map(|s| &s.id) == Some(&t.id);
                let unread = self.unread.contains_key(&t.id);
                div()
                    .id(SharedString::from(format!("t-{}", t.id)))
                    .h(px(28.))
                    .ml(px(18.))
                    .px_2()
                    .rounded(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .cursor_pointer()
                    .text_sm()
                    .text_color(rgb(if active || unread {
                        color::bright()
                    } else {
                        color::muted()
                    }))
                    .when(active, |d| d.bg(rgb(color::active())))
                    .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
                    .child("⤷")
                    .child(div().flex_1().overflow_hidden().child(t.title()))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_channel(chan.clone(), cx)),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn forum_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(forum) = &self.channel else {
            return div().into_any_element();
        };
        let mut posts: Vec<Channel> = self
            .threads
            .iter()
            .filter(|t| t.parent_id.as_deref() == Some(&forum.id))
            .cloned()
            .collect();
        for t in self.forum_archived.get(&forum.id).into_iter().flatten() {
            if !posts.iter().any(|p| p.id == t.id) {
                posts.push(t.clone());
            }
        }
        posts.sort_by_key(|p| std::cmp::Reverse(p.id.parse::<u64>().unwrap_or(0)));
        let mut list = div()
            .id("forum")
            .flex_1()
            .overflow_y_scroll()
            .p_4()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                div()
                    .text_size(px(20.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(color::bright()))
                    .child("Publications"),
            )
            .child(div().text_sm().text_color(rgb(color::muted())).child(
                "Écrivez le titre, Maj+Entrée, puis le message, pour créer une publication.",
            ));
        if posts.is_empty() {
            list = list.child(
                div()
                    .mt_4()
                    .text_color(rgb(color::muted()))
                    .child("Aucune publication pour le moment."),
            );
        }
        for p in posts {
            let archived = p.thread_metadata.as_ref().is_some_and(|m| m.archived);
            let count = p.message_count.unwrap_or(0);
            let chan = p.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("post-{}", p.id)))
                    .p_3()
                    .rounded(px(8.))
                    .bg(rgb(color::sidebar()))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::hover())))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color::bright()))
                            .child(p.title()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(color::muted()))
                            .child(format!(
                                "{count} message{}{}",
                                if count > 1 { "s" } else { "" },
                                if archived { " · Archivé" } else { "" }
                            )),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_channel(chan.clone(), cx)),
                    ),
            );
        }
        list.into_any_element()
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
            .text_color(rgb(if bright {
                color::bright()
            } else {
                color::muted()
            }))
            .when(unread.is_some(), |d| d.font_weight(FontWeight::SEMIBOLD))
            .when(active, |d| d.bg(rgb(color::active())))
            .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
            .child(
                div()
                    .text_size(px(20.))
                    .child(if c.is_forum() { "▤" } else { "#" }),
            )
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
            .bg(rgb(color::panel()))
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
                            color::green()
                        } else {
                            color::muted()
                        }))
                        .border_2()
                        .border_color(rgb(color::panel())),
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
                            .text_color(rgb(color::bright()))
                            .child(me.display_name().to_string()),
                    )
                    .child(div().text_xs().text_color(rgb(color::muted())).child(
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
                    .text_color(rgb(color::muted()))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
                    .child("Réglages")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.settings = true;
                        cx.notify();
                    })),
            )
    }

    // ---- chat pane -----------------------------------------------------

    fn chat(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (prefix, title) = match &self.channel {
            Some(c) if c.is_dm() => ("@", c.title()),
            Some(c) if c.is_thread() => ("⤷", c.title()),
            Some(c) if c.is_forum() => ("▤", c.title()),
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
            .border_color(rgb(color::rail()))
            .when(self.width < 760., |d| {
                d.child(
                    div()
                        .id("nav-toggle")
                        .px_2()
                        .py_1()
                        .rounded(px(4.))
                        .text_size(px(18.))
                        .cursor_pointer()
                        .text_color(rgb(color::muted()))
                        .hover(|d| d.bg(rgb(color::hover())))
                        .child("☰")
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.nav_open = !this.nav_open;
                            cx.notify();
                        })),
                )
            })
            .child(
                div()
                    .text_size(px(22.))
                    .text_color(rgb(color::muted()))
                    .child(prefix),
            )
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color::bright()))
                    .child(title.clone()),
            )
            .when(!topic.is_empty(), |d| {
                d.child(div().w(px(1.)).h(px(24.)).mx_1().bg(rgb(color::divider())))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .text_sm()
                            .text_color(rgb(color::muted()))
                            .child(topic),
                    )
            })
            .child(div().flex_1())
            .when(self.channel.is_some(), |d| {
                let side = self.show_side;
                d.child(self.header_button(
                    "pins-btn",
                    "Épingles",
                    side && self.side == Side::Pins,
                    cx,
                    |t, cx| t.open_pins(cx),
                ))
                .child(self.header_button(
                    "members-btn",
                    "Membres",
                    side && self.side == Side::Members,
                    cx,
                    |t, cx| t.show_members_panel(cx),
                ))
                .child(self.header_button(
                    "search-btn",
                    "Rechercher",
                    side && self.side == Side::Search,
                    cx,
                    |t, cx| {
                        t.search = Some(String::new());
                        t.side = Side::Search;
                        t.show_side = true;
                        cx.notify();
                    },
                ))
            });

        let forbidden = self
            .channel
            .as_ref()
            .is_some_and(|c| self.forbidden.contains(&c.id));
        let body: AnyElement = if forbidden {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .px_4()
                .text_center()
                .child(div().text_size(px(40.)).child("🔒"))
                .child(
                    div()
                        .text_size(px(20.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(color::bright()))
                        .child("Vous n'avez pas accès à ce salon"),
                )
                .child(
                    div()
                        .text_color(rgb(color::muted()))
                        .child("Vous n'avez pas la permission de voir les messages ici (403)."),
                )
                .into_any_element()
        } else if self.channel.as_ref().is_some_and(|c| c.is_forum()) {
            self.forum_view(cx)
        } else if self.channel.is_some() && self.loading_messages && self.messages.is_empty() {
            self.messages_skeleton()
        } else if self.channel.is_some() {
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
                    .text_color(rgb(color::link()))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::hover())))
                    .child(if self.loading_older {
                        "Chargement…"
                    } else {
                        "Charger les messages précédents"
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.load_older(cx)))
            });
            let rows = self.message_rows(cx);
            let away = {
                let max = self.scroll.max_offset().height;
                max > px(0.) && self.scroll.offset().y < -max + px(160.)
            };
            let jump = away.then(|| {
                div()
                    .id("jump-bottom")
                    .absolute()
                    .bottom(px(12.))
                    .right(px(24.))
                    .px_3()
                    .py_1()
                    .rounded_full()
                    .bg(rgb(color::brand()))
                    .shadow_lg()
                    .cursor_pointer()
                    .text_sm()
                    .text_color(rgb(0xffffff))
                    .child("Aller aux derniers messages ↓")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.scroll.scroll_to_bottom();
                        cx.notify();
                    }))
            });
            div()
                .relative()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .child(
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
                        .child(div().h(px(16.)).flex_shrink_0()),
                )
                .children(jump)
                .into_any_element()
        } else if self.guild.is_none() {
            self.friends_view(cx)
        } else {
            div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(rgb(color::muted()))
                .child("Choisissez un salon.")
                .into_any_element()
        };

        let placeholder = if self.channel.as_ref().is_some_and(|c| c.is_forum()) {
            "Titre de la publication (Maj+Entrée pour le message)".to_string()
        } else if prefix == "@" {
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
        let slash = self.slash_popup(cx);

        let cannot_send = self
            .channel
            .as_ref()
            .is_some_and(|c| self.no_send.contains(&c.id) || self.forbidden.contains(&c.id));
        let locked = (self.channel.is_some() && cannot_send).then(|| {
            div().px_4().pb(px(24.)).flex_shrink_0().child(
                div()
                    .min_h(px(44.))
                    .px_4()
                    .rounded(px(8.))
                    .bg(rgb(color::input()))
                    .opacity(0.7)
                    .flex()
                    .items_center()
                    .text_color(rgb(color::muted()))
                    .child(
                        "🔒  Vous n'avez pas la permission d'envoyer des messages dans ce salon.",
                    ),
            )
        });
        let composer = (self.channel.is_some() && !cannot_send).then(|| {
            div()
                .px_4()
                .flex_shrink_0()
                .flex()
                .flex_col()
                .children(slash)
                .children(banner)
                .child(
                    div()
                        .min_h(px(44.))
                        .py(px(10.))
                        .px_4()
                        .when(has_banner, |d| d.rounded_b(px(8.)))
                        .when(!has_banner, |d| d.rounded(px(8.)))
                        .bg(rgb(color::input()))
                        .flex()
                        .items_start()
                        .gap_3()
                        .child(
                            div()
                                .id("upload")
                                .size(px(24.))
                                .flex_shrink_0()
                                .rounded_full()
                                .bg(rgb(color::muted()))
                                .hover(|d| d.bg(rgb(color::bright())))
                                .cursor_pointer()
                                .text_color(rgb(color::input()))
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
                                    this.picker_tab = PickerTab::Emoji;
                                    this.gif_typing = false;
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
                        .text_color(rgb(color::bright()))
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
                        .text_color(rgb(color::red()))
                        .child(self.status.clone()),
                )
            })
            .children(composer)
            .children(locked);

        let picker = self.picker.is_some().then(|| self.emoji_picker(cx));
        div()
            .id("chat")
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .bg(rgb(color::chat()))
            .flex()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.upload(paths.paths().to_vec(), cx)
            }))
            .child(main)
            .when(
                self.show_side && self.channel.is_some() && self.width >= 1050.,
                |d| d.child(self.side_panel(cx)),
            )
            .when(
                self.show_side && self.channel.is_some() && self.width < 1050.,
                |d| {
                    d.child(
                        div()
                            .absolute()
                            .top_0()
                            .right_0()
                            .h_full()
                            .shadow_lg()
                            .child(self.side_panel(cx)),
                    )
                },
            )
            .children(picker)
    }

    fn messages_skeleton(&self) -> AnyElement {
        div()
            .flex_1()
            .overflow_hidden()
            .p_4()
            .flex()
            .flex_col()
            .justify_end()
            .gap_4()
            .children((0..7).map(|i| {
                let w1 = [110., 150., 90., 130., 170., 100., 140.][i % 7];
                let w2: f32 = [380., 260., 460., 320., 220., 420., 300.][i % 7];
                div()
                    .flex()
                    .gap_4()
                    .child(skeleton(format!("sk-m-av-{i}"), Some(40.), 40., None))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_2()
                            .child(skeleton(format!("sk-m-n-{i}"), Some(w1), 12., Some(6.)))
                            .child(skeleton(
                                format!("sk-m-t-{i}"),
                                Some(w2.min(self.width - 160.)),
                                14.,
                                Some(6.),
                            )),
                    )
            }))
            .into_any_element()
    }

    fn picker_tabs(&self, cx: &mut Context<Self>) -> Div {
        let composer = matches!(self.picker, Some(Picker::Composer));
        let tab = |id: &'static str, label: &'static str, active: bool| {
            div()
                .id(id)
                .px_3()
                .py_1()
                .rounded(px(4.))
                .text_sm()
                .cursor_pointer()
                .text_color(rgb(if active {
                    color::bright()
                } else {
                    color::muted()
                }))
                .when(active, |d| d.bg(rgb(color::active())))
                .hover(|d| d.bg(rgb(color::hover())))
                .child(label)
        };
        div()
            .p_2()
            .flex()
            .gap_1()
            .child(
                tab("tab-emoji", "Émojis", self.picker_tab == PickerTab::Emoji).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.picker_tab = PickerTab::Emoji;
                        this.gif_typing = false;
                        cx.notify();
                    }),
                ),
            )
            .when(composer, |d| {
                d.child(
                    tab("tab-gif", "GIF", self.picker_tab == PickerTab::Gif)
                        .on_click(cx.listener(|this, _, _, cx| this.open_gif_tab(cx))),
                )
            })
    }

    fn gif_body(&self, cx: &mut Context<Self>) -> AnyElement {
        let search = div()
            .mx_2()
            .h(px(32.))
            .px_3()
            .rounded(px(4.))
            .bg(rgb(color::rail()))
            .flex()
            .items_center()
            .child(if self.gif_query.is_empty() {
                div().flex().items_center().child(Self::caret()).child(
                    div()
                        .text_color(rgb(color::muted()))
                        .child("Rechercher des GIF (Entrée)"),
                )
            } else {
                div()
                    .flex()
                    .items_center()
                    .child(self.gif_query.clone())
                    .child(Self::caret())
            });
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .gap_2()
            .child(search)
            .child(
                div()
                    .id("gif-scroll")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .flex()
                    .flex_col()
                    .when(!self.gif_note.is_empty(), |d| {
                        d.child(
                            div()
                                .text_sm()
                                .text_color(rgb(color::muted()))
                                .child(self.gif_note.clone()),
                        )
                    })
                    .child(div().flex().flex_wrap().gap_1().children(
                        self.gifs.iter().enumerate().map(|(i, g)| {
                            let url = g.url.clone();
                            let tile = div()
                                .id(SharedString::from(format!("gif-{i}")))
                                .w(px(108.))
                                .h(px(84.))
                                .rounded(px(4.))
                                .overflow_hidden()
                                .bg(rgb(color::rail()))
                                .cursor_pointer()
                                .hover(|d| d.opacity(0.8));
                            match self.images.get(&g.preview) {
                                Some(im) => tile.child(
                                    img(im.clone())
                                        .w(px(108.))
                                        .h(px(84.))
                                        .object_fit(ObjectFit::Cover),
                                ),
                                None => tile.flex().items_center().justify_center().child(
                                    div()
                                        .text_xs()
                                        .text_color(rgb(color::muted()))
                                        .child(g.title.chars().take(14).collect::<String>()),
                                ),
                            }
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.send_gif(url.clone(), cx)),
                            )
                        }),
                    )),
            )
            .into_any_element()
    }

    fn emoji_picker(&self, cx: &mut Context<Self>) -> AnyElement {
        let composer = matches!(self.picker, Some(Picker::Composer));
        let mut body = div()
            .id("emoji-scroll")
            .flex_1()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_2();
        if let Some(custom) = self.guild.as_ref().and_then(|g| self.guild_emojis.get(g)) {
            if !custom.is_empty() {
                body = body
                    .child(
                        div()
                            .text_xs()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(color::muted()))
                            .child("CE SERVEUR"),
                    )
                    .child(div().flex().flex_wrap().children(custom.iter().map(|e| {
                        let emoji = e.clone();
                        let pic: AnyElement = match self.images.get(&e.url()) {
                            Some(i) => img(i.clone()).size(px(28.)).into_any_element(),
                            None => div().text_xs().child(e.name.clone()).into_any_element(),
                        };
                        div()
                            .id(SharedString::from(format!("ce-{}", e.id)))
                            .size(px(36.))
                            .rounded(px(4.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .cursor_pointer()
                            .hover(|d| d.bg(rgb(color::hover())))
                            .child(pic)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.pick_custom(&emoji, cx)),
                            )
                    })));
            }
        }
        for (name, list) in crate::emoji::CATEGORIES {
            body = body
                .child(
                    div()
                        .text_xs()
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(color::muted()))
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
                        .hover(|d| d.bg(rgb(color::hover())))
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
                    .w(px(360.0_f32.min(self.width - 32.)))
                    .h(px(340.))
                    .rounded(px(8.))
                    .bg(rgb(color::sidebar()))
                    .border_1()
                    .border_color(rgb(color::rail()))
                    .shadow_lg()
                    .flex()
                    .flex_col()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(self.picker_tabs(cx))
                    .child(if self.picker_tab == PickerTab::Gif && composer {
                        self.gif_body(cx)
                    } else {
                        body.into_any_element()
                    }),
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
                .text_color(rgb(color::muted()))
                .child("Aucun ami à afficher pour le moment.")
                .into_any_element();
        }
        let mut sorted_friends = self.friends.clone();
        sorted_friends.sort_by_key(|u| {
            let rank = match self.presence.get(&u.id).map(|s| s.as_str()) {
                Some("online") => 0,
                Some("idle") => 1,
                Some("dnd") => 2,
                _ => 3,
            };
            (rank, u.display_name().to_lowercase())
        });
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
                    .text_color(rgb(color::muted()))
                    .child(format!("AMIS — {}", self.friends.len())),
            )
            .children(sorted_friends.iter().map(|u| {
                let user = u.clone();
                let st = self
                    .presence
                    .get(&u.id)
                    .map(|s| s.as_str())
                    .unwrap_or("offline");
                div()
                    .id(SharedString::from(format!("f-{}", u.id)))
                    .h(px(60.))
                    .px_3()
                    .rounded(px(8.))
                    .border_t_1()
                    .border_color(rgb(color::divider()))
                    .flex()
                    .items_center()
                    .gap_3()
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::hover())))
                    .child(self.avatar_status(u, 36., color::chat()))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(color::bright()))
                                    .child(u.display_name().to_string()),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(color::muted()))
                                    .child(format!("{} · {}", status_label(st), u.username)),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open_dm_with(&user, cx)))
            }))
            .into_any_element()
    }

    fn slash_popup(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let sugg = self.slash_suggestions();
        if sugg.is_empty() {
            return None;
        }
        let sel = self.slash_sel.min(sugg.len() - 1);
        Some(
            div()
                .mb_1()
                .p_2()
                .rounded(px(8.))
                .bg(rgb(color::sidebar()))
                .border_1()
                .border_color(rgb(color::divider()))
                .shadow_lg()
                .flex()
                .flex_col()
                .child(Self::section_label("COMMANDES".into()))
                .children(sugg.into_iter().enumerate().map(|(i, e)| {
                    let name = e.name.clone();
                    div()
                        .id(SharedString::from(format!("slash-{i}")))
                        .h(px(36.))
                        .px_2()
                        .rounded(px(4.))
                        .flex()
                        .items_center()
                        .gap_3()
                        .cursor_pointer()
                        .when(i == sel, |d| d.bg(rgb(color::active())))
                        .hover(|d| d.bg(rgb(color::hover())))
                        .child(
                            div()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(rgb(color::bright()))
                                .child(format!("/{}", e.name)),
                        )
                        .child(
                            div()
                                .flex_1()
                                .overflow_hidden()
                                .text_sm()
                                .text_color(rgb(color::muted()))
                                .child(e.desc),
                        )
                        .child(div().text_xs().text_color(rgb(color::muted())).child(e.app))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.set_input(format!("/{name} "));
                            cx.notify();
                        }))
                }))
                .into_any_element(),
        )
    }

    fn banner(&self, text: String, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px_4()
            .py(px(6.))
            .rounded_t(px(8.))
            .bg(rgb(color::sidebar()))
            .flex()
            .items_center()
            .text_sm()
            .text_color(rgb(color::muted()))
            .child(div().flex_1().overflow_hidden().child(text))
            .child(
                div()
                    .id("cancel-compose")
                    .px_2()
                    .cursor_pointer()
                    .hover(|d| d.text_color(rgb(color::bright())))
                    .child("✕")
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.cancel_compose();
                        cx.notify();
                    })),
            )
            .into_any_element()
    }

    fn side_panel(&self, cx: &mut Context<Self>) -> AnyElement {
        match self.side {
            Side::Members => self.members_panel(cx).into_any_element(),
            Side::Search => self.search_panel(cx).into_any_element(),
            Side::Pins => self
                .result_panel(
                    "MESSAGES ÉPINGLÉS",
                    &self.pins,
                    "Aucun message épinglé.",
                    cx,
                )
                .into_any_element(),
        }
    }

    fn side_shell(&self, id: &'static str) -> Stateful<Div> {
        div()
            .id(id)
            .w(px(260.))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(color::sidebar()))
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap(px(2.))
    }

    fn section_label(text: String) -> Div {
        div()
            .px_2()
            .pt_3()
            .pb_1()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(color::muted()))
            .child(text)
    }

    fn person_row(&self, u: &User, status: Option<&str>, cx: &mut Context<Self>) -> Stateful<Div> {
        let status = status.or_else(|| self.presence.get(&u.id).map(|s| s.as_str()));
        let offline = matches!(status, Some("offline") | Some("invisible"));
        let dot = status.and_then(status_color);
        let tint = self.name_color(&u.id);
        let user = u.clone();
        div()
            .id(SharedString::from(format!("p-{}", u.id)))
            .h(px(42.))
            .px_2()
            .flex()
            .items_center()
            .gap_3()
            .rounded(px(4.))
            .cursor_pointer()
            .when(offline, |d| d.opacity(0.5))
            .hover(|d| d.bg(rgb(color::hover())))
            .child(
                div()
                    .relative()
                    .child(self.avatar(u, 32.))
                    .children(dot.map(|c| {
                        div()
                            .absolute()
                            .right(px(-2.))
                            .bottom(px(-2.))
                            .size(px(12.))
                            .rounded_full()
                            .bg(rgb(c))
                            .border_2()
                            .border_color(rgb(color::sidebar()))
                    })),
            )
            .child(
                div()
                    .flex_1()
                    .overflow_hidden()
                    .text_color(rgb(tint.unwrap_or_else(color::muted)))
                    .child(u.display_name().to_string()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.profile = Some(user.clone());
                cx.notify();
            }))
    }

    fn members_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let mut panel = self.side_shell("members");
        let in_guild = self.channel.as_ref().is_some_and(|c| !c.is_dm());
        if in_guild && !self.members.is_empty() {
            for row in &self.members {
                match row {
                    MemberRow::Group { id, count } => {
                        let name = match id.as_str() {
                            "online" => "EN LIGNE".to_string(),
                            "offline" => "HORS LIGNE".to_string(),
                            other => self
                                .roles
                                .get(other)
                                .map(|r| r.name.to_uppercase())
                                .unwrap_or_else(|| "RÔLE".into()),
                        };
                        panel = panel.child(Self::section_label(format!("{name} — {count}")));
                    }
                    MemberRow::Member { user, status, .. } => {
                        panel = panel.child(self.person_row(user, Some(status), cx));
                    }
                }
            }
            return panel;
        }
        // Fallback: DM participants / recent authors.
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
        panel = panel.child(Self::section_label(format!("{label} — {}", people.len())));
        for u in &people {
            panel = panel.child(self.person_row(u, None, cx));
        }
        panel
    }

    fn search_panel(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let focused = self.search.is_some();
        let query = self.search.clone().unwrap_or_default();
        let input = div()
            .id("search-box")
            .h(px(34.))
            .px_3()
            .mb_2()
            .rounded(px(4.))
            .bg(rgb(color::rail()))
            .border_1()
            .border_color(rgb(if focused {
                color::brand()
            } else {
                color::rail()
            }))
            .flex()
            .items_center()
            .cursor_text()
            .overflow_hidden()
            .child(if focused {
                div()
                    .flex()
                    .items_center()
                    .child(query)
                    .child(Self::caret())
            } else {
                div()
                    .text_color(rgb(color::muted()))
                    .child("Rechercher (Entrée pour lancer)")
            })
            .on_click(cx.listener(|this, _, _, cx| {
                if this.search.is_none() {
                    this.search = Some(String::new());
                }
                cx.notify();
            }));
        let mut panel = self
            .side_shell("search")
            .child(Self::section_label("RECHERCHE".into()))
            .child(input);
        if !self.search_note.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .text_sm()
                    .text_color(rgb(color::muted()))
                    .child(self.search_note.clone()),
            );
        }
        for m in &self.search_results {
            panel = panel.child(self.result_row(m));
        }
        panel
    }

    fn result_panel(
        &self,
        title: &str,
        msgs: &[Message],
        empty: &str,
        _cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let mut panel = self
            .side_shell("results")
            .child(Self::section_label(format!("{title} — {}", msgs.len())));
        if msgs.is_empty() {
            panel = panel.child(
                div()
                    .px_2()
                    .text_sm()
                    .text_color(rgb(color::muted()))
                    .child(empty.to_string()),
            );
        }
        for m in msgs {
            panel = panel.child(self.result_row(m));
        }
        panel
    }

    fn result_row(&self, m: &Message) -> Div {
        let when = parse_time(&m.timestamp)
            .map(|t| stamp(&t))
            .unwrap_or_default();
        let (text, _) = markdown(&m.content, false);
        let text: String = text.chars().take(240).collect();
        div()
            .mb_1()
            .p_2()
            .rounded(px(4.))
            .bg(rgb(color::chat()))
            .flex()
            .gap_2()
            .child(self.avatar(&m.author, 28.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .flex()
                            .items_baseline()
                            .gap_2()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(color::bright()))
                                    .child(m.author.display_name().to_string()),
                            )
                            .child(
                                div()
                                    .text_size(px(10.))
                                    .text_color(rgb(color::muted()))
                                    .child(when),
                            ),
                    )
                    .child(div().text_sm().child(if text.is_empty() {
                        "(pièce jointe)".to_string()
                    } else {
                        text
                    })),
            )
    }

    fn header_button(
        &self,
        id: &'static str,
        label: &'static str,
        active: bool,
        cx: &mut Context<Self>,
        on: impl Fn(&mut Self, &mut Context<Self>) + 'static,
    ) -> Stateful<Div> {
        div()
            .id(id)
            .px_2()
            .py_1()
            .rounded(px(4.))
            .text_sm()
            .cursor_pointer()
            .text_color(rgb(if active {
                color::bright()
            } else {
                color::muted()
            }))
            .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
            .child(label)
            .on_click(cx.listener(move |this, _, _, cx| on(this, cx)))
    }

    fn profile_overlay(&self, user: &User, cx: &mut Context<Self>) -> AnyElement {
        let u = user.clone();
        let (u2, id) = (user.clone(), user.id.clone());
        let is_me = self.me.as_ref().is_some_and(|m| m.id == user.id);
        let button = |label: &'static str, id: &'static str, primary: bool| {
            div()
                .id(id)
                .h(px(36.))
                .px_4()
                .rounded(px(3.))
                .flex()
                .items_center()
                .cursor_pointer()
                .text_sm()
                .text_color(rgb(0xffffff))
                .bg(rgb(if primary {
                    color::brand()
                } else {
                    color::active()
                }))
                .child(label)
        };
        div()
            .id("profile-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(rgba(0x000000a0))
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, _, cx| {
                this.profile = None;
                cx.notify();
            }))
            .child(
                div()
                    .id("profile")
                    .w(px(360.0_f32.min(self.width - 32.)))
                    .rounded(px(8.))
                    .bg(rgb(color::sidebar()))
                    .shadow_lg()
                    .overflow_hidden()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(div().h(px(80.)).bg(rgb(color::brand())))
                    .child(
                        div()
                            .px_4()
                            .pb_4()
                            .flex()
                            .flex_col()
                            .gap_3()
                            .child(
                                div()
                                    .mt(px(-40.))
                                    .size(px(88.))
                                    .rounded_full()
                                    .border_4()
                                    .border_color(rgb(color::sidebar()))
                                    .child(self.avatar(&u, 80.)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .text_size(px(20.))
                                            .font_weight(FontWeight::BOLD)
                                            .text_color(rgb(color::bright()))
                                            .child(u.display_name().to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_color(rgb(color::muted()))
                                            .child(u.username.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_xs()
                                    .text_color(rgb(color::muted()))
                                    .child(format!("ID : {}", u.id)),
                            )
                            .child(
                                div()
                                    .flex()
                                    .gap_2()
                                    .when(!is_me, |d| {
                                        d.child(
                                            button("Envoyer un message", "profile-dm", true)
                                                .on_click(cx.listener(move |this, _, _, cx| {
                                                    this.profile = None;
                                                    if this.guild.is_some() {
                                                        this.guild = None;
                                                        this.channels.clear();
                                                    }
                                                    this.open_dm_with(&u2, cx);
                                                })),
                                        )
                                    })
                                    .child(button("Copier l'ID", "profile-copy", false).on_click(
                                        cx.listener(move |_, _, _, cx| {
                                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(
                                                id.clone(),
                                            ))
                                        }),
                                    )),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn settings_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let me = self.me.clone().unwrap_or_default();
        let toggle = |id: &'static str, label: &'static str, on: bool| {
            div()
                .id(id)
                .h(px(40.))
                .flex()
                .items_center()
                .justify_between()
                .cursor_pointer()
                .child(label)
                .child(
                    div()
                        .w(px(40.))
                        .h(px(22.))
                        .rounded_full()
                        .bg(rgb(if on { color::green() } else { color::muted() }))
                        .flex()
                        .items_center()
                        .when(on, |d| d.justify_end())
                        .px(px(3.))
                        .child(div().size(px(16.)).rounded_full().bg(rgb(0xffffff))),
                )
        };
        div()
            .id("settings-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(rgba(0x000000a0))
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings = false;
                cx.notify();
            }))
            .child(
                div()
                    .id("settings")
                    .w(px(480.0_f32.min(self.width - 32.)))
                    .rounded(px(8.))
                    .bg(rgb(color::chat()))
                    .shadow_lg()
                    .p_5()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .on_click(|_, _, cx| cx.stop_propagation())
                    .child(
                        div()
                            .text_size(px(20.))
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(color::bright()))
                            .child("Paramètres utilisateur"),
                    )
                    .child(Self::section_label("MON COMPTE".into()))
                    .child(
                        div()
                            .p_3()
                            .rounded(px(8.))
                            .bg(rgb(color::sidebar()))
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(self.avatar(&me, 48.))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .child(
                                        div()
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .text_color(rgb(color::bright()))
                                            .child(me.display_name().to_string()),
                                    )
                                    .child(
                                        div()
                                            .text_sm()
                                            .text_color(rgb(color::muted()))
                                            .child(me.username.clone()),
                                    ),
                            ),
                    )
                    .child(Self::section_label("APPLICATION".into()))
                    .child(
                        toggle("set-theme", "Thème clair", self.light)
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_theme(cx))),
                    )
                    .child(
                        toggle("set-notif", "Notifications du bureau", self.notifications)
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.notifications = !this.notifications;
                                this.save_prefs();
                                cx.notify();
                            })),
                    )
                    .child(
                        toggle("set-side", "Afficher le panneau latéral", self.show_side).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.show_side = !this.show_side;
                                this.save_prefs();
                                cx.notify();
                            }),
                        ),
                    )
                    .child(
                        toggle(
                            "set-title",
                            "Afficher les mentions dans le titre de la fenêtre",
                            self.title_badge,
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.title_badge = !this.title_badge;
                            this.save_prefs();
                            cx.notify();
                        })),
                    )
                    .child(
                        div()
                            .mt_2()
                            .flex()
                            .gap_2()
                            .child(
                                div()
                                    .id("set-logout")
                                    .h(px(36.))
                                    .px_4()
                                    .rounded(px(3.))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .bg(rgb(color::red()))
                                    .text_sm()
                                    .text_color(rgb(0xffffff))
                                    .child("Se déconnecter")
                                    .on_click(cx.listener(|this, _, _, cx| this.logout(cx))),
                            )
                            .child(
                                div()
                                    .id("set-close")
                                    .h(px(36.))
                                    .px_4()
                                    .rounded(px(3.))
                                    .flex()
                                    .items_center()
                                    .cursor_pointer()
                                    .bg(rgb(color::active()))
                                    .text_sm()
                                    .text_color(rgb(0xffffff))
                                    .child("Fermer")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.settings = false;
                                        cx.notify();
                                    })),
                            ),
                    ),
            )
            .into_any_element()
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
                    .text_color(rgb(color::bright()))
                    .child(if prefix == "@" {
                        title.to_string()
                    } else if prefix == "⤷" {
                        title.to_string()
                    } else {
                        format!("Bienvenue sur #{title} !")
                    }),
            )
            .child(div().text_color(rgb(color::muted())).child(if prefix == "@" {
                format!("Ceci est le début de votre historique de messages privés avec @{title}.")
            } else if prefix == "⤷" {
                "Ceci est le début du fil de discussion.".to_string()
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
        let line = || div().flex_1().h(px(1.)).bg(rgb(color::divider()));
        div()
            .mx_4()
            .my_2()
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(color::muted()))
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
                    color: Some(rgb(color::muted()).into()),
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
                None if self.image_failed.contains(&a.preview_url()) => div()
                    .px_3()
                    .py_2()
                    .rounded(px(4.))
                    .bg(rgb(color::sidebar()))
                    .text_color(rgb(color::link()))
                    .child(a.filename.clone())
                    .into_any_element(),
                None => skeleton(format!("sk-att-{}", a.url), Some(w), h, Some(8.)),
            };
        }
        div()
            .px_3()
            .py_2()
            .rounded(px(4.))
            .bg(rgb(color::sidebar()))
            .text_color(rgb(color::link()))
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
                        color::link()
                    } else {
                        color::bright()
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
                            .text_color(rgb(color::bright()))
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
            .bg(rgb(color::sidebar()))
            .border_l_4()
            .border_color(rgb(e.color.unwrap_or(color::divider())))
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
                    .border_color(rgb(if me { color::brand() } else { color::sidebar() }))
                    .bg(rgb(if me {
                        color::react_me()
                    } else {
                        color::sidebar()
                    }))
                    .hover(|d| d.border_color(rgb(color::divider())))
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
                .text_color(rgb(if danger { color::red() } else { color::muted() }))
                .hover(|d| d.bg(rgb(color::hover())))
                .child(label.to_string())
        };
        let (reply, edit, del, react, thread) =
            (m.clone(), m.clone(), m.clone(), m.clone(), m.clone());
        let can_thread = m.thread.is_none()
            && self
                .channel
                .as_ref()
                .is_some_and(|c| !c.is_dm() && !c.is_thread());
        let copy = m.content.clone();
        div()
            .absolute()
            .top(px(-14.))
            .right(px(16.))
            .flex()
            .rounded(px(4.))
            .border_1()
            .border_color(rgb(color::rail()))
            .bg(rgb(color::chat()))
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
            .when(can_thread, |d| {
                d.child(
                    btn(format!("thread-{}", m.id), "Fil", false).on_click(
                        cx.listener(move |this, _, _, cx| this.start_thread(&thread, cx)),
                    ),
                )
            })
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
                    .text_color(rgb(self
                        .name_color(&m.author.id)
                        .unwrap_or_else(color::bright)))
                    .child(name.clone()),
            );
            if let Some(t) = time {
                head = head.child(
                    div()
                        .text_xs()
                        .text_color(rgb(color::muted()))
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
        if let Some(t) = &m.thread {
            let chan = (**t).clone();
            let count = t.message_count.unwrap_or(0);
            text = text.child(
                div()
                    .id(SharedString::from(format!("chip-{}", m.id)))
                    .mt_1()
                    .max_w(px(420.))
                    .px_3()
                    .py_1()
                    .rounded(px(6.))
                    .bg(rgb(color::sidebar()))
                    .border_1()
                    .border_color(rgb(color::divider()))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::hover())))
                    .flex()
                    .items_center()
                    .gap_2()
                    .text_sm()
                    .child("⤷")
                    .child(
                        div()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color::link()))
                            .child(t.title()),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(rgb(color::muted()))
                            .child(format!(
                                "{count} message{}",
                                if count > 1 { "s" } else { "" }
                            )),
                    )
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_channel(chan.clone(), cx)),
                    ),
            );
        }

        let author = m.author.clone();
        let gutter = if grouped {
            div()
                .w(px(40.))
                .flex_shrink_0()
                .flex()
                .justify_center()
                .text_size(px(10.))
                .text_color(rgb(color::muted()))
                .opacity(0.)
                .group_hover(group.clone(), |s| s.opacity(1.))
                .child(time.map(hhmm).unwrap_or_default())
                .into_any_element()
        } else {
            let a = author.clone();
            div()
                .id(SharedString::from(format!("av-{}", m.id)))
                .cursor_pointer()
                .child(self.avatar(&m.author, 40.))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.profile = Some(a.clone());
                    cx.notify();
                }))
                .into_any_element()
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
                d.bg(rgb(color::ping_bg()))
                    .border_l_2()
                    .border_color(rgb(0xf0b232))
            })
            .px_4()
            .py(px(2.))
            .when(!grouped, |d| d.mt(px(14.)))
            .hover(|d| d.bg(rgb(color::msg_hover())))
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
                    .text_color(rgb(color::muted()))
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
