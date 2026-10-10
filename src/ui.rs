//! Discord-style layout: server rail, channel sidebar, chat pane, login screen.

use chrono::{DateTime, Datelike, Local, Timelike};
use std::ops::Range;
use std::sync::Arc;

use gpui::{
    div, ease_in_out, img, linear_color_stop, linear_gradient, prelude::*, pulsating_between, px,
    rgb, rgba, svg, Animation, AnimationExt, AnyElement, AnyView, App, Context, Div, ExternalPaths,
    FontStyle, FontWeight, HighlightStyle, Image, InteractiveText, KeyDownEvent, MouseButton,
    MouseMoveEvent, ObjectFit, Render, SharedString, Stateful, StrikethroughStyle, StyledImage,
    StyledText, Svg, UnderlineStyle, Window, WindowControlArea,
};

use crate::api;
use crate::api::{Channel, Message, User};
use crate::gateway::MemberRow;
use crate::{DiscordApp, Field, LoginMode, Picker, PickerTab, QrState, Side};

/// Theme palette (Discord 2025 refresh: Light, Ash, Dark, Onyx).
pub mod color {
    use std::sync::atomic::{AtomicU8, Ordering};

    #[derive(Clone, Copy, PartialEq, Eq, Debug)]
    pub enum Theme {
        Light = 0,
        Ash = 1,
        Dark = 2,
        Onyx = 3,
    }

    impl Theme {
        pub const ALL: [Theme; 4] = [Theme::Light, Theme::Ash, Theme::Dark, Theme::Onyx];

        pub fn label(self) -> &'static str {
            match self {
                Theme::Light => "Clair",
                Theme::Ash => "Cendre",
                Theme::Dark => "Sombre",
                Theme::Onyx => "Onyx",
            }
        }

        pub fn key(self) -> &'static str {
            match self {
                Theme::Light => "light",
                Theme::Ash => "ash",
                Theme::Dark => "dark",
                Theme::Onyx => "onyx",
            }
        }

        pub fn from_key(k: &str) -> Theme {
            Theme::ALL
                .into_iter()
                .find(|t| t.key() == k)
                .unwrap_or(Theme::Dark)
        }
    }

    static THEME: AtomicU8 = AtomicU8::new(Theme::Dark as u8);

    pub fn set_theme(t: Theme) {
        THEME.store(t as u8, Ordering::Relaxed);
    }

    pub fn theme() -> Theme {
        Theme::ALL[THEME.load(Ordering::Relaxed) as usize % 4]
    }

    pub fn is_light() -> bool {
        theme() == Theme::Light
    }

    /// Token table, indexed by [`Theme`].
    const T: [[u32; 20]; 4] = [
        // Light
        [
            0xe3e5e8, 0xf2f3f5, 0xffffff, 0xffffff, 0xebedef, 0xe8e9ec, 0xdcdee2, 0x2e3338,
            0x060607, 0x5c5e66, 0x006ce7, 0xe1e2e4, 0xf7f7f8, 0xfff6dc, 0xe3e6fc, 0x3c45a5,
            0xebedef, 0xe3e5e8, 0xdcdde0, 0x111214,
        ],
        // Ash
        [
            0x1e1f22, 0x2b2d31, 0x232428, 0x313338, 0x383a40, 0x35373c, 0x404249, 0xdbdee1,
            0xf2f3f5, 0x949ba4, 0x00a8fc, 0x3f4147, 0x2e3035, 0x444037, 0x3b405a, 0xc9cdfb,
            0x1e1f22, 0x1e1f22, 0x3a3c41, 0x111214,
        ],
        // Dark
        [
            0x121214, 0x121214, 0x1e1e22, 0x1a1a1e, 0x222327, 0x1f1f23, 0x29292e, 0xdfe0e2,
            0xfbfbfb, 0x8d8e96, 0x4ea8ff, 0x26262b, 0x1e1e22, 0x2e2a1e, 0x262a45, 0xc9cdfb,
            0x121214, 0x121214, 0x26262b, 0x0b0b0c,
        ],
        // Onyx
        [
            0x000000, 0x000000, 0x0f0f11, 0x070708, 0x111113, 0x111113, 0x1c1c1f, 0xdcdde0,
            0xffffff, 0x84858c, 0x4ea8ff, 0x1a1a1d, 0x0e0e10, 0x241f12, 0x1b1e36, 0xc9cdfb,
            0x121214, 0x000000, 0x1c1c1f, 0x18181b,
        ],
    ];

    fn tok(i: usize) -> u32 {
        T[theme() as usize][i]
    }

    pub fn rail() -> u32 {
        tok(0)
    }

    pub fn sidebar() -> u32 {
        tok(1)
    }

    pub fn panel() -> u32 {
        tok(2)
    }

    pub fn chat() -> u32 {
        tok(3)
    }

    pub fn input() -> u32 {
        tok(4)
    }

    pub fn hover() -> u32 {
        tok(5)
    }

    pub fn active() -> u32 {
        tok(6)
    }

    pub fn text() -> u32 {
        tok(7)
    }

    pub fn bright() -> u32 {
        tok(8)
    }

    pub fn muted() -> u32 {
        tok(9)
    }

    pub fn link() -> u32 {
        tok(10)
    }

    pub fn divider() -> u32 {
        tok(11)
    }

    pub fn msg_hover() -> u32 {
        tok(12)
    }

    pub fn ping_bg() -> u32 {
        tok(13)
    }

    pub fn react_me() -> u32 {
        tok(14)
    }

    pub fn mention_fg() -> u32 {
        tok(15)
    }

    pub fn code_bg() -> u32 {
        tok(16)
    }

    pub fn frame() -> u32 {
        tok(17)
    }

    pub fn border() -> u32 {
        tok(18)
    }

    pub fn tooltip() -> u32 {
        tok(19)
    }

    pub fn brand() -> u32 {
        0x5865f2
    }

    pub fn brand_hover() -> u32 {
        0x4752c4
    }

    pub fn green() -> u32 {
        0x23a55a
    }

    pub fn yellow() -> u32 {
        0xf0b232
    }

    pub fn red() -> u32 {
        if is_light() {
            0xda373c
        } else {
            0xf23f42
        }
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

/// Public user flags shown as text badges when the profile endpoint gives none.
const FLAG_BADGES: [(u64, &str); 10] = [
    (1 << 0, "Staff Discord"),
    (1 << 1, "Partenaire"),
    (1 << 2, "HypeSquad Events"),
    (1 << 3, "Chasseur de bugs"),
    (1 << 6, "HypeSquad Bravery"),
    (1 << 7, "HypeSquad Brilliance"),
    (1 << 8, "HypeSquad Balance"),
    (1 << 9, "Soutien précoce"),
    (1 << 17, "Développeur de bots vérifié"),
    (1 << 18, "Modérateur certifié"),
];

fn connection_label(kind: &str) -> String {
    match kind {
        "twitter" => "X (Twitter)".into(),
        "youtube" => "YouTube".into(),
        "github" => "GitHub".into(),
        "reddit" => "Reddit".into(),
        "spotify" => "Spotify".into(),
        "steam" => "Steam".into(),
        "twitch" => "Twitch".into(),
        "xbox" => "Xbox".into(),
        "playstation" => "PlayStation".into(),
        "epicgames" => "Epic Games".into(),
        "battlenet" => "Battle.net".into(),
        "tiktok" => "TikTok".into(),
        "instagram" => "Instagram".into(),
        "facebook" => "Facebook".into(),
        "bluesky" => "Bluesky".into(),
        "domain" => "Domaine".into(),
        other => {
            let mut c = other.chars();
            c.next()
                .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
                .unwrap_or_default()
        }
    }
}

// ---- design primitives ---------------------------------------------------------

/// Lucide icon tinted with `color`.
pub fn icon(name: &str, size: f32, color: u32) -> Svg {
    svg()
        .path(SharedString::from(format!("icons/{name}.svg")))
        .size(px(size))
        .flex_none()
        .text_color(rgb(color))
}

/// Small tooltip bubble (used through `.tooltip(tip("…"))`).
pub struct Tip(SharedString);

impl Render for Tip {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div()
            .font_family(crate::assets::FONT_FAMILY)
            .px_2()
            .py(px(5.))
            .rounded(px(6.))
            .bg(rgb(color::tooltip()))
            .shadow_md()
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0xf2f3f5))
            .child(self.0.clone())
    }
}

pub fn tip(text: impl Into<SharedString>) -> impl Fn(&mut Window, &mut App) -> AnyView + 'static {
    let text: SharedString = text.into();
    move |_, cx| cx.new(|_| Tip(text.clone())).into()
}

/// Square icon button with hover state and tooltip.
pub fn icon_button(
    id: impl Into<SharedString>,
    name: &str,
    label: impl Into<SharedString>,
    active: bool,
) -> Stateful<Div> {
    let id: SharedString = id.into();
    let group: SharedString = format!("ib-{id}").into();
    div()
        .id(id)
        .group(group.clone())
        .size(px(32.))
        .flex_none()
        .rounded(px(8.))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .when(active, |d| d.bg(rgb(color::active())))
        .hover(|d| d.bg(rgb(color::hover())))
        .child(
            icon(
                name,
                20.,
                if active {
                    color::bright()
                } else {
                    color::muted()
                },
            )
            .group_hover(group, |s| s.text_color(rgb(color::bright()))),
        )
        .tooltip(tip(label))
}

/// Marks an element as a native window-control hit area (custom title bar on Windows).
pub fn control_area<E: InteractiveElement>(mut el: E, area: WindowControlArea) -> E {
    el.interactivity().window_control_area(area);
    el
}

/// Minimise / maximise / close buttons for the custom title bar (Windows).
pub fn window_buttons() -> Div {
    let btn = |id: &'static str, glyph: &'static str, area: WindowControlArea, danger: bool| {
        let g: SharedString = format!("wb-{id}").into();
        control_area(
            div()
                .id(id)
                .group(g.clone())
                .w(px(46.))
                .h_full()
                .flex()
                .items_center()
                .justify_center()
                .when(danger, |d| d.hover(|d| d.bg(rgb(0xe81123))))
                .when(!danger, |d| d.hover(|d| d.bg(rgb(color::hover()))))
                .child(icon(glyph, 16., color::muted()).group_hover(g, |s| {
                    s.text_color(rgb(if danger { 0xffffff } else { color::bright() }))
                })),
            area,
        )
    };
    div()
        .h_full()
        .flex()
        .child(btn("win-min", "minus", WindowControlArea::Min, false))
        .child(btn("win-max", "square", WindowControlArea::Max, false))
        .child(btn("win-close", "x", WindowControlArea::Close, true))
}

/// Fade + slide-in used by popovers and dialogs.
pub fn appear<E: IntoElement + Styled + 'static>(id: impl Into<SharedString>, el: E) -> AnyElement {
    let id: SharedString = id.into();
    el.with_animation(
        id,
        Animation::new(std::time::Duration::from_millis(160)).with_easing(ease_in_out),
        |el, d| el.opacity(d),
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

// ---- rich message text: mentions, roles, channels, timestamps -----------------

const MARK_OPEN: char = '\u{E000}';
const MARK_CLOSE: char = '\u{E001}';
const MARK_SEP: char = '\u{E002}';

const WEEKDAYS: [&str; 7] = [
    "lundi", "mardi", "mercredi", "jeudi", "vendredi", "samedi", "dimanche",
];

fn plural(n: i64, unit: &str) -> String {
    format!(
        "{n} {unit}{}",
        if n > 1 && !unit.ends_with('s') {
            "s"
        } else {
            ""
        }
    )
}

/// Discord `<t:secs:style>` rendering.
fn fmt_timestamp(secs: i64, style: &str) -> String {
    let Some(dt) = DateTime::from_timestamp(secs, 0).map(|d| d.with_timezone(&Local)) else {
        return "date invalide".into();
    };
    let date = format!(
        "{} {} {}",
        dt.day(),
        MONTHS[dt.month0() as usize],
        dt.year()
    );
    let hm = hhmm(&dt);
    match style {
        "t" => hm,
        "T" => format!("{hm}:{:02}", dt.second()),
        "d" => format!("{:02}/{:02}/{}", dt.day(), dt.month(), dt.year()),
        "D" => date,
        "F" => format!(
            "{} {date} {hm}",
            WEEKDAYS[dt.weekday().num_days_from_monday() as usize]
        ),
        "R" => {
            let diff = Local::now().timestamp() - secs;
            let (n, future) = (diff.abs(), diff < 0);
            let text = if n < 60 {
                plural(n, "seconde")
            } else if n < 3600 {
                plural(n / 60, "minute")
            } else if n < 86_400 {
                plural(n / 3600, "heure")
            } else if n < 86_400 * 30 {
                plural(n / 86_400, "jour")
            } else if n < 86_400 * 365 {
                format!("{} mois", n / (86_400 * 30))
            } else {
                plural(n / (86_400 * 365), "an")
            };
            if future {
                format!("dans {text}")
            } else {
                format!("il y a {text}")
            }
        }
        _ => format!("{date} {hm}"),
    }
}

fn fmt_date_ms(ms: u64) -> String {
    match DateTime::from_timestamp_millis(ms as i64).map(|d| d.with_timezone(&Local)) {
        Some(d) => format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year()),
        None => String::new(),
    }
}

fn fmt_date_iso(ts: &str) -> String {
    parse_time(ts)
        .map(|d| format!("{} {} {}", d.day(), MONTHS[d.month0() as usize], d.year()))
        .unwrap_or_default()
}

/// Like [`resolve_tokens`] but wraps mentions in private-use markers so they can be
/// styled and made clickable after Markdown has run.
/// Marker layout: OPEN kind id SEP label CLOSE.
fn resolve_rich(src: &str, lookup: &dyn Fn(char, &str) -> Option<String>) -> String {
    let mut out = String::with_capacity(src.len());
    let mut rest = src;
    let wrap = |kind: char, id: &str, label: String| {
        format!("{MARK_OPEN}{kind}{id}{MARK_SEP}{label}{MARK_CLOSE}")
    };
    while let Some(start) = rest.find('<') {
        out.push_str(&rest[..start]);
        let tail = &rest[start..];
        let Some(end) = tail.find('>') else {
            out.push_str(tail);
            return out;
        };
        let inner = &tail[1..end];
        let replaced = if let Some(id) = inner.strip_prefix("@&") {
            Some(wrap(
                'r',
                id,
                lookup('r', id).unwrap_or_else(|| "@rôle".into()),
            ))
        } else if let Some(id) = inner.strip_prefix("@!").or_else(|| inner.strip_prefix('@')) {
            Some(wrap(
                'u',
                id,
                lookup('u', id).unwrap_or_else(|| "@utilisateur".into()),
            ))
        } else if let Some(id) = inner.strip_prefix('#') {
            Some(wrap(
                'c',
                id,
                lookup('c', id).unwrap_or_else(|| "#salon".into()),
            ))
        } else if let Some(t) = inner.strip_prefix("t:") {
            lookup('t', t).map(|label| wrap('t', t, label))
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

#[derive(Clone)]
pub enum Action {
    Url(String),
    User(String),
    Channel(String),
}

struct Span {
    range: Range<usize>,
    kind: char,
    id: String,
}

/// Strips the markers left by [`resolve_rich`], remapping highlight ranges and
/// returning the mention spans in the cleaned text.
fn extract_mentions(
    text: &str,
    hl: Vec<(Range<usize>, HighlightStyle)>,
) -> (String, Vec<(Range<usize>, HighlightStyle)>, Vec<Span>) {
    let mut out = String::with_capacity(text.len());
    let mut map = vec![0usize; text.len() + 1];
    let mut spans = Vec::new();
    let mut i = 0;
    while i < text.len() {
        let c = text[i..].chars().next().unwrap();
        let cl = c.len_utf8();
        if c == MARK_OPEN {
            if let Some(e1) = text[i..].find(MARK_CLOSE) {
                let seg = &text[i + cl..i + e1];
                if let Some(e2) = seg.find(MARK_SEP) {
                    let head = &seg[..e2];
                    let label = &seg[e2 + MARK_SEP.len_utf8()..];
                    let kind = head.chars().next().unwrap_or('u');
                    let id = head[kind.len_utf8()..].to_string();
                    let end = i + e1 + MARK_CLOSE.len_utf8();
                    for b in map.iter_mut().take(end).skip(i) {
                        *b = out.len();
                    }
                    let start = out.len();
                    out.push_str(label);
                    spans.push(Span {
                        range: start..out.len(),
                        kind,
                        id,
                    });
                    i = end;
                    continue;
                }
            }
        }
        for b in 0..cl {
            map[i + b] = out.len() + b;
        }
        out.push(c);
        i += cl;
    }
    map[text.len()] = out.len();
    let hl = hl
        .into_iter()
        .filter_map(|(r, s)| {
            let (a, b) = (map[r.start.min(text.len())], map[r.end.min(text.len())]);
            (a < b).then_some((a..b, s))
        })
        .collect();
    // Literal @everyone / @here.
    for word in ["@everyone", "@here"] {
        let mut from = 0;
        while let Some(p) = out[from..].find(word) {
            let s = from + p;
            spans.push(Span {
                range: s..s + word.len(),
                kind: 'e',
                id: String::new(),
            });
            from = s + word.len();
        }
    }
    spans.sort_by_key(|s| s.range.start);
    (out, hl, spans)
}

fn overlaps(a: &Range<usize>, b: &Range<usize>) -> bool {
    a.start < b.end && b.start < a.end
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
        // Mention markers are opaque: never interpret Markdown inside them.
        if rest.starts_with(MARK_OPEN) {
            if let Some(end) = rest.find(MARK_CLOSE) {
                let end = end + MARK_CLOSE.len_utf8();
                out.push_str(&rest[..end]);
                rest = &rest[end..];
                continue 'outer;
            }
        }
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
        .right(px(12.))
        .bottom(px(-4.))
        .min_w(px(18.))
        .h(px(18.))
        .px(px(5.))
        .rounded_full()
        .bg(rgb(color::red()))
        .border_2()
        .border_color(rgb(color::frame()))
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
    div()
        .relative()
        .w_full()
        .flex_shrink_0()
        .flex()
        .justify_center()
        .child(
            div()
                .absolute()
                .left_0()
                .top(if active { px(4.) } else { px(16.) })
                .w(px(4.))
                .h(px(if active { 32. } else { 8. }))
                .rounded_r(px(4.))
                .bg(rgb(color::bright()))
                .when(!active && !unread, |d| d.opacity(0.)),
        )
        .child(button)
        .when(mentions > 0, |d| d.child(badge(mentions)))
}

/// Squircle server button: icon image, or initials.
fn rail_button(
    id: impl Into<SharedString>,
    active: bool,
    label: String,
    image: Option<Arc<Image>>,
) -> Stateful<Div> {
    let base = div()
        .id(id.into())
        .size(px(40.))
        .rounded(px(12.))
        .overflow_hidden()
        .cursor_pointer();
    if let Some(image) = image {
        return base.child(round_image(image, 40., Some(12.)));
    }
    base.flex()
        .items_center()
        .justify_center()
        .text_color(rgb(if active { 0xffffff } else { color::bright() }))
        .font_weight(FontWeight::SEMIBOLD)
        .text_size(px(14.))
        .child(label)
        .when(active, |d| d.bg(rgb(color::brand())))
        .when(!active, |d| {
            d.bg(rgb(color::active()))
                .hover(|h| h.bg(rgb(color::brand())))
        })
}

impl DiscordApp {
    pub fn root(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let narrow = self.width < 760.;
        let body: AnyElement = if !self.logged_in {
            self.login_view(cx).into_any_element()
        } else {
            let content = if narrow {
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
                                .shadow_lg()
                                .on_click(|_, _, cx| cx.stop_propagation())
                                .child(self.nav_column(cx)),
                        )
                });
                div()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .flex()
                    .child(self.chat(cx))
                    .children(nav)
            } else {
                div()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(self.nav_column(cx))
                    .child(self.chat(cx))
            };
            div()
                .size_full()
                .flex()
                .flex_col()
                .bg(rgb(color::frame()))
                .child(self.title_bar(cx))
                .child(content)
                .into_any_element()
        };
        let switcher = self.switcher.is_some().then(|| self.switcher_overlay(cx));
        let profile = self.profile.clone().map(|u| self.profile_overlay(&u, cx));
        let settings = self.settings.then(|| self.settings_overlay(cx));
        let inbox = self.inbox_open.then(|| self.inbox_popover(cx));
        let status = self.status_menu.then(|| self.status_popover(cx));
        let size = match self.density.as_str() {
            "compact" => 14.,
            "spacious" => 16.,
            _ => 15.,
        };
        div()
            .id("root")
            .track_focus(&self.focus)
            .on_key_down(cx.listener(|this, ev: &KeyDownEvent, _, cx| this.on_key(ev, cx)))
            .on_mouse_move(cx.listener(|this, ev: &MouseMoveEvent, _, cx| {
                if this.resizing {
                    this.sidebar_width = (f32::from(ev.position.x) - 72.).clamp(180., 420.);
                    cx.notify();
                }
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _, _, cx| {
                    if this.resizing {
                        this.resizing = false;
                        this.save_prefs();
                        cx.notify();
                    }
                }),
            )
            .when(self.resizing, |d| d.cursor_col_resize())
            .relative()
            .size_full()
            .font_family(crate::assets::FONT_FAMILY)
            .bg(rgb(color::frame()))
            .text_color(rgb(color::text()))
            .text_size(px(size))
            .child(body)
            .children(inbox)
            .children(status)
            .children(profile)
            .children(settings)
            .children(switcher)
    }

    /// Thin top bar (2025 layout): current place in the middle, inbox on the right.
    fn title_bar(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let (glyph, title) = match (&self.guild, &self.channel) {
            (None, None) => ("users", "Amis".to_string()),
            (None, Some(c)) => ("at-sign", c.title()),
            (Some(g), _) => (
                "house",
                self.guilds
                    .iter()
                    .find(|x| &x.id == g)
                    .map(|x| x.name.clone())
                    .unwrap_or_default(),
            ),
        };
        let mentions: u32 = self.unread.values().map(|u| u.mentions).sum();
        let custom = cfg!(any(target_os = "windows", target_os = "macos"));
        control_area(
            div()
                .id("titlebar")
                .h(px(36.))
                .flex_shrink_0()
                .pl(px(if cfg!(target_os = "macos") { 80. } else { 8. }))
                .flex()
                .items_center()
                .on_mouse_down(MouseButton::Left, |ev, window, _| {
                    if ev.click_count == 2 {
                        window.zoom_window();
                    }
                }),
            WindowControlArea::Drag,
        )
        .child(div().flex_1())
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .text_size(px(13.))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(color::bright()))
                .child(icon(glyph, 14., color::muted()))
                .child(title),
        )
        .child(
            div()
                .flex_1()
                .flex()
                .justify_end()
                .items_center()
                .gap_1()
                .child(
                    div()
                        .relative()
                        .child(
                            icon_button(
                                "inbox-btn",
                                "inbox",
                                "Boîte de réception",
                                self.inbox_open,
                            )
                            .on_click(cx.listener(|this, _, _, cx| this.toggle_inbox(cx))),
                        )
                        .when(mentions > 0, |d| {
                            d.child(
                                div()
                                    .absolute()
                                    .top(px(2.))
                                    .right(px(2.))
                                    .size(px(8.))
                                    .rounded_full()
                                    .bg(rgb(color::red())),
                            )
                        }),
                )
                .child(
                    icon_button("switch-btn", "search", "Changement rapide (Ctrl+K)", false)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.switcher = Some(String::new());
                            this.switcher_sel = 0;
                            cx.notify();
                        })),
                )
                .when(custom && cfg!(target_os = "windows"), |d| {
                    d.child(div().w(px(8.))).child(window_buttons())
                }),
        )
    }

    /// Server rail + channel sidebar + floating user panel.
    fn nav_column(&mut self, cx: &mut Context<Self>) -> Div {
        let narrow = self.width < 760.;
        let sw = if narrow { 240. } else { self.sidebar_width };
        div()
            .relative()
            .h_full()
            .flex_shrink_0()
            .flex()
            .bg(rgb(color::frame()))
            .child(self.rail(cx))
            .child(self.sidebar(sw, cx))
            .when(!narrow, |d| {
                d.child(
                    div()
                        .id("resize")
                        .absolute()
                        .top_0()
                        .right(px(-3.))
                        .w(px(6.))
                        .h_full()
                        .cursor_col_resize()
                        .hover(|d| d.bg(rgba(0x5865f240)))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.resizing = true;
                                cx.notify();
                            }),
                        ),
                )
            })
            .child(
                div()
                    .absolute()
                    .left(px(8.))
                    .bottom(px(8.))
                    .w(px(72. + sw - 16.))
                    .child(self.user_panel(cx)),
            )
    }

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
            div()
                .id("home")
                .size(px(40.))
                .rounded(px(12.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(home_active, |d| d.bg(rgb(color::brand())))
                .when(!home_active, |d| {
                    d.bg(rgb(color::active()))
                        .hover(|d| d.bg(rgb(color::brand())))
                })
                .child(icon(
                    "message-circle",
                    22.,
                    if home_active {
                        0xffffff
                    } else {
                        color::bright()
                    },
                ))
                .tooltip(tip("Messages privés"))
                .on_click(cx.listener(|this, _, _, cx| this.open_home(cx))),
            home_active,
            dm_unread,
            dm_mentions,
        );
        let guilds = self.guilds.iter().map(|g| {
            let active = self.guild.as_deref() == Some(&g.id);
            let id = g.id.clone();
            let img_ = g.icon_url().and_then(|u| self.images.get(&u).cloned());
            let mine = || {
                self.unread
                    .values()
                    .filter(|u| u.guild.as_deref() == Some(&g.id))
            };
            let (unread, mentions) = (mine().next().is_some(), mine().map(|u| u.mentions).sum());
            rail_slot(
                rail_button(format!("g-{}", g.id), active, acronym(&g.name), img_)
                    .tooltip(tip(g.name.clone()))
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
            .pt_1()
            .pb(px(72.))
            .flex()
            .flex_col()
            .items_center()
            .gap_2()
            .overflow_y_scroll()
            .child(home)
            .child(
                div()
                    .w(px(24.))
                    .h(px(1.))
                    .flex_shrink_0()
                    .bg(rgb(color::divider())),
            )
            .children(guilds)
    }

    // ---- channel sidebar -----------------------------------------------

    fn sidebar(&mut self, width: f32, cx: &mut Context<Self>) -> impl IntoElement {
        let header: AnyElement = match &self.guild {
            None => div()
                .h(px(48.))
                .flex_shrink_0()
                .px_2()
                .flex()
                .items_center()
                .border_b_1()
                .border_color(rgb(color::border()))
                .child(
                    div()
                        .id("dm-search")
                        .flex_1()
                        .h(px(30.))
                        .px_2()
                        .rounded(px(8.))
                        .bg(rgb(color::input()))
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_pointer()
                        .text_size(px(13.))
                        .text_color(rgb(color::muted()))
                        .child(icon("search", 14., color::muted()))
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child("Rechercher une conversation"),
                        )
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.switcher = Some(String::new());
                            this.switcher_sel = 0;
                            cx.notify();
                        })),
                )
                .into_any_element(),
            Some(id) => {
                let name = self
                    .guilds
                    .iter()
                    .find(|g| &g.id == id)
                    .map(|g| g.name.clone())
                    .unwrap_or_default();
                div()
                    .id("guild-header")
                    .h(px(48.))
                    .flex_shrink_0()
                    .px_4()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(color::border()))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::hover())))
                    .child(
                        div()
                            .flex_1()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_color(rgb(color::bright()))
                            .font_weight(FontWeight::SEMIBOLD)
                            .child(name),
                    )
                    .child(icon("chevron-down", 18., color::bright()))
                    .into_any_element()
            }
        };

        let rows: Vec<AnyElement> = if self.guild.is_none() {
            self.dm_rows(cx)
        } else if self.channels.is_empty() {
            (0..9)
                .map(|i| {
                    div()
                        .px_2()
                        .py(px(7.))
                        .child(skeleton(
                            format!("sk-ch-{i}"),
                            Some([140., 100., 160., 120., 90., 150., 110., 130., 100.][i]),
                            12.,
                            Some(6.),
                        ))
                        .into_any_element()
                })
                .collect()
        } else {
            self.channel_rows(cx)
        };

        div()
            .w(px(width))
            .h_full()
            .flex_shrink_0()
            .bg(rgb(color::sidebar()))
            .rounded_tl(px(12.))
            .border_t_1()
            .border_l_1()
            .border_color(rgb(color::border()))
            .overflow_hidden()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div()
                    .id("channel-list")
                    .flex_1()
                    .overflow_y_scroll()
                    .px_2()
                    .pt_2()
                    .pb(px(72.))
                    .flex()
                    .flex_col()
                    .gap(px(1.))
                    .children(rows),
            )
    }

    fn user_panel(&self, cx: &mut Context<Self>) -> Div {
        let me = self.me.clone().unwrap_or_default();
        let st = if self.connected {
            self.my_status.as_str()
        } else {
            "offline"
        };
        let dot = status_color(st).unwrap_or_else(color::muted);
        div()
            .h(px(56.))
            .px_2()
            .rounded(px(10.))
            .bg(rgb(color::panel()))
            .border_1()
            .border_color(rgb(color::border()))
            .shadow_md()
            .flex()
            .items_center()
            .gap_2()
            .child(
                div()
                    .id("me-card")
                    .flex_1()
                    .min_w_0()
                    .h(px(44.))
                    .px_1()
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::hover())))
                    .child(
                        div().relative().child(self.avatar(&me, 32.)).child(
                            div()
                                .absolute()
                                .right(px(-2.))
                                .bottom(px(-2.))
                                .size(px(12.))
                                .rounded_full()
                                .bg(rgb(dot))
                                .border_2()
                                .border_color(rgb(color::panel())),
                        ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(rgb(color::bright()))
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .overflow_hidden()
                                    .child(me.display_name().to_string()),
                            )
                            .child(div().text_xs().text_color(rgb(color::muted())).child(
                                if self.connected {
                                    status_label(st)
                                } else {
                                    "Hors connexion"
                                },
                            )),
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.status_menu = !this.status_menu;
                        this.inbox_open = false;
                        cx.notify();
                    })),
            )
            .child(
                icon_button("settings-btn", "settings", "Paramètres utilisateur", false).on_click(
                    cx.listener(|this, _, _, cx| {
                        this.settings = true;
                        cx.notify();
                    }),
                ),
            )
    }

    fn status_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let options = [
            ("online", "En ligne", ""),
            ("idle", "Inactif", ""),
            (
                "dnd",
                "Ne pas déranger",
                "Vous ne recevrez pas de notifications sur le bureau.",
            ),
            (
                "invisible",
                "Invisible",
                "Vous apparaîtrez hors ligne, avec tous les accès.",
            ),
        ];
        let panel = div()
            .id("status-menu")
            .absolute()
            .left(px(16.))
            .bottom(px(72.))
            .w(px(260.))
            .p_2()
            .rounded(px(10.))
            .bg(rgb(color::panel()))
            .border_1()
            .border_color(rgb(color::border()))
            .shadow_lg()
            .flex()
            .flex_col()
            .gap_1()
            .on_click(|_, _, cx| cx.stop_propagation())
            .children(options.into_iter().map(|(key, label, help)| {
                let dot = status_color(key).unwrap_or_else(color::muted);
                div()
                    .id(SharedString::from(format!("st-{key}")))
                    .px_2()
                    .py(px(6.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(self.my_status == key, |d| d.bg(rgb(color::active())))
                    .hover(|d| d.bg(rgb(color::hover())))
                    .flex()
                    .gap_2()
                    .child(
                        div()
                            .mt(px(5.))
                            .size(px(10.))
                            .flex_shrink_0()
                            .rounded_full()
                            .when(key == "invisible", |d| {
                                d.border_2().border_color(rgb(color::muted()))
                            })
                            .when(key != "invisible", |d| d.bg(rgb(dot))),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(color::bright()))
                                    .child(label),
                            )
                            .when(!help.is_empty(), |d| {
                                d.child(div().text_xs().text_color(rgb(color::muted())).child(help))
                            }),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.set_status(key, cx)))
            }));
        div()
            .id("status-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|this, _, _, cx| {
                this.status_menu = false;
                cx.notify();
            }))
            .child(appear("status-anim", panel))
            .into_any_element()
    }

    fn inbox_popover(&self, cx: &mut Context<Self>) -> AnyElement {
        let mut list = div()
            .id("inbox-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_2()
            .flex()
            .flex_col()
            .gap_2();
        if self.inbox_loading {
            for i in 0..4 {
                list = list.child(skeleton(format!("sk-in-{i}"), None, 56., Some(8.)));
            }
        } else if self.inbox.is_empty() {
            list = list.child(
                div()
                    .py_6()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_2()
                    .text_color(rgb(color::muted()))
                    .child(icon("inbox", 40., color::muted()))
                    .child("Aucune mention récente."),
            );
        }
        for m in &self.inbox {
            let place = match &m.guild_id {
                Some(g) => self
                    .guilds
                    .iter()
                    .find(|x| &x.id == g)
                    .map(|x| x.name.clone())
                    .unwrap_or_else(|| "Serveur".into()),
                None => "Message privé".into(),
            };
            let msg = m.clone();
            list = list.child(
                div()
                    .id(SharedString::from(format!("in-{}", m.id)))
                    .rounded(px(8.))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::hover())))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .px_2()
                            .pt_1()
                            .flex()
                            .items_center()
                            .gap_1()
                            .text_xs()
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(color::muted()))
                            .child(icon("hash", 12., color::muted()))
                            .child(place),
                    )
                    .child(self.result_row(m))
                    .on_click(cx.listener(move |this, _, _, cx| this.open_message(&msg, cx))),
            );
        }
        let panel = div()
            .id("inbox")
            .absolute()
            .top(px(40.))
            .right(px(8.))
            .w(px(420.0_f32.min(self.width - 16.)))
            .h(px((self.height - 80.).clamp(240., 560.)))
            .rounded(px(10.))
            .bg(rgb(color::panel()))
            .border_1()
            .border_color(rgb(color::border()))
            .shadow_lg()
            .flex()
            .flex_col()
            .on_click(|_, _, cx| cx.stop_propagation())
            .child(
                div()
                    .px_4()
                    .py_3()
                    .flex()
                    .items_center()
                    .gap_2()
                    .border_b_1()
                    .border_color(rgb(color::border()))
                    .child(icon("at-sign", 18., color::bright()))
                    .child(
                        div()
                            .font_weight(FontWeight::BOLD)
                            .text_color(rgb(color::bright()))
                            .child("Mentions"),
                    ),
            )
            .child(list);
        div()
            .id("inbox-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .on_click(cx.listener(|this, _, _, cx| {
                this.inbox_open = false;
                cx.notify();
            }))
            .child(appear("inbox-anim", panel))
            .into_any_element()
    }

    fn switcher_overlay(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let items = self.switcher_items();
        let sel = self.switcher_sel.min(items.len().saturating_sub(1));
        let query = self.switcher.clone().unwrap_or_default();
        appear(
            "switcher-backdrop-anim",
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
                ),
        )
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
                .child(div().w_full().text_center().child(match &self.qr {
                    QrState::Failed(_) => "Impossible de générer le code QR.".to_string(),
                    QrState::Scanned(_) => "Code scanné".to_string(),
                    _ => "Génération du code…".to_string(),
                }))
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

    // ---- channel sidebar -----------------------------------------------

    fn dm_rows(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let friends_active = self.channel.is_none();
        let mut rows: Vec<AnyElement> = vec![
            div()
                .id("friends-entry")
                .h(px(40.))
                .px_2()
                .rounded(px(8.))
                .flex()
                .items_center()
                .gap_3()
                .cursor_pointer()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(if friends_active {
                    color::bright()
                } else {
                    color::muted()
                }))
                .when(friends_active, |d| d.bg(rgb(color::active())))
                .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
                .child(icon(
                    "users",
                    20.,
                    if friends_active {
                        color::bright()
                    } else {
                        color::muted()
                    },
                ))
                .child("Amis")
                .on_click(cx.listener(|this, _, _, cx| this.open_home(cx)))
                .into_any_element(),
            div()
                .px_2()
                .pt_4()
                .pb_1()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(rgb(color::muted()))
                .child("Messages privés")
                .into_any_element(),
        ];
        for c in &self.dms {
            let active = self.channel.as_ref().map(|s| &s.id) == Some(&c.id);
            let chan = c.clone();
            let title = c.title();
            let pic = match c.recipients.first() {
                Some(u) if c.recipients.len() == 1 => self.avatar_status(
                    u,
                    32.,
                    if active {
                        color::active()
                    } else {
                        color::sidebar()
                    },
                ),
                Some(u) => self.avatar(u, 32.),
                None => initials_avatar(&c.id, &title, 32.).into_any_element(),
            };
            let unread = self.unread.get(&c.id).cloned();
            let bright = active || unread.is_some();
            let sub =
                (c.recipients.len() > 1).then(|| format!("{} membres", c.recipients.len() + 1));
            rows.push(
                div()
                    .id(SharedString::from(format!("dm-{}", c.id)))
                    .h(px(44.))
                    .px_2()
                    .rounded(px(8.))
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
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(title),
                            )
                            .children(
                                sub.map(|s| {
                                    div().text_xs().text_color(rgb(color::muted())).child(s)
                                }),
                            ),
                    )
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

    fn category_row(&self, cat: &Channel, cx: &mut Context<Self>) -> AnyElement {
        let collapsed = self.collapsed.contains(&cat.id);
        let cid = cat.id.clone();
        let group: SharedString = format!("cat-g-{}", cat.id).into();
        div()
            .id(SharedString::from(format!("cat-{}", cat.id)))
            .group(group.clone())
            .mt_4()
            .mb_1()
            .px_1()
            .flex()
            .items_center()
            .gap_1()
            .cursor_pointer()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(color::muted()))
            .hover(|d| d.text_color(rgb(color::bright())))
            .child(
                icon(
                    if collapsed {
                        "chevron-right"
                    } else {
                        "chevron-down"
                    },
                    12.,
                    color::muted(),
                )
                .group_hover(group, |s| s.text_color(rgb(color::bright()))),
            )
            .child(
                div()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(cat.title().to_uppercase()),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                if !this.collapsed.remove(&cid) {
                    this.collapsed.insert(cid.clone());
                }
                cx.notify();
            }))
            .into_any_element()
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
            rows.push(self.category_row(cat, cx));
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
                    .h(px(30.))
                    .ml(px(16.))
                    .px_2()
                    .rounded(px(8.))
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
                    .child(icon("message-square-text", 15., color::muted()))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(t.title()),
                    )
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
        let glyph = match c.kind {
            5 => "megaphone",
            15 | 16 => "messages-square",
            _ => "hash",
        };
        let tint = if bright {
            color::bright()
        } else {
            color::muted()
        };
        let group: SharedString = format!("ch-g-{}", c.id).into();
        div()
            .id(SharedString::from(format!("c-{}", c.id)))
            .group(group.clone())
            .relative()
            .h(px(32.))
            .px_2()
            .rounded(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .cursor_pointer()
            .text_color(rgb(tint))
            .when(unread.is_some(), |d| d.font_weight(FontWeight::SEMIBOLD))
            .when(active, |d| d.bg(rgb(color::active())))
            .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
            .when(unread.is_some() && !active, |d| {
                d.child(
                    div()
                        .absolute()
                        .left(px(-8.))
                        .top(px(12.))
                        .w(px(4.))
                        .h(px(8.))
                        .rounded_r(px(4.))
                        .bg(rgb(color::bright())),
                )
            })
            .child(
                icon(glyph, 18., tint).group_hover(group, |s| s.text_color(rgb(color::bright()))),
            )
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .child(c.title()),
            )
            .children(
                unread
                    .filter(|u| u.mentions > 0)
                    .map(|u| mention_pill(u.mentions)),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select_channel(chan.clone(), cx)))
            .into_any_element()
    }

    // ---- chat pane -----------------------------------------------------

    fn chat(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let (glyph, prefix, title) = match &self.channel {
            Some(c) if c.is_dm() => ("at-sign", "@", c.title()),
            Some(c) if c.is_thread() => ("message-square-text", "⤷", c.title()),
            Some(c) if c.is_forum() => ("messages-square", "▤", c.title()),
            Some(c) if c.kind == 5 => ("megaphone", "#", c.title()),
            Some(c) => ("hash", "#", c.title()),
            None if self.guild.is_none() => ("users", "", "Amis".to_string()),
            None => ("hash", "", String::new()),
        };
        let topic = self
            .channel
            .as_ref()
            .and_then(|c| c.topic.clone())
            .unwrap_or_default();
        let compact_header = self.width < 900.;

        let side = self.show_side;
        let header = div()
            .h(px(48.))
            .flex_shrink_0()
            .px_3()
            .flex()
            .items_center()
            .gap_2()
            .border_b_1()
            .border_color(rgb(color::border()))
            .when(self.width < 760., |d| {
                d.child(
                    icon_button("nav-toggle", "menu", "Navigation", self.nav_open).on_click(
                        cx.listener(|this, _, _, cx| {
                            this.nav_open = !this.nav_open;
                            cx.notify();
                        }),
                    ),
                )
            })
            .child(icon(glyph, 22., color::muted()))
            .child(
                div()
                    .flex_shrink_0()
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(color::bright()))
                    .child(title.clone()),
            )
            .when(!topic.is_empty() && !compact_header, |d| {
                d.child(div().w(px(1.)).h(px(20.)).mx_1().bg(rgb(color::divider())))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_sm()
                            .text_color(rgb(color::muted()))
                            .child(topic),
                    )
            })
            .child(div().flex_1())
            .when(self.channel.is_some(), |d| {
                d.child(
                    icon_button(
                        "pins-btn",
                        "pin",
                        "Messages épinglés",
                        side && self.side == Side::Pins,
                    )
                    .on_click(cx.listener(|t, _, _, cx| t.open_pins(cx))),
                )
                .child(
                    icon_button(
                        "members-btn",
                        "users",
                        "Afficher la liste des membres",
                        side && self.side == Side::Members,
                    )
                    .on_click(cx.listener(|t, _, _, cx| t.show_members_panel(cx))),
                )
                .child(
                    div()
                        .id("search-btn")
                        .ml_1()
                        .w(px(if compact_header { 32. } else { 180. }))
                        .h(px(28.))
                        .px_2()
                        .rounded(px(8.))
                        .bg(rgb(color::frame()))
                        .border_1()
                        .border_color(rgb(if side && self.side == Side::Search {
                            color::brand()
                        } else {
                            color::border()
                        }))
                        .flex()
                        .items_center()
                        .gap_2()
                        .cursor_text()
                        .text_size(px(13.))
                        .text_color(rgb(color::muted()))
                        .when(!compact_header, |d| {
                            d.child(div().flex_1().child("Rechercher"))
                        })
                        .child(icon("search", 15., color::muted()))
                        .tooltip(tip("Rechercher (Ctrl+F)"))
                        .on_click(cx.listener(|t, _, _, cx| {
                            t.search = Some(String::new());
                            t.side = Side::Search;
                            t.show_side = true;
                            cx.notify();
                        })),
                )
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
                .gap_3()
                .px_4()
                .text_center()
                .child(
                    div()
                        .size(px(72.))
                        .rounded_full()
                        .bg(rgb(color::input()))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon("lock", 32., color::muted())),
                )
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
                    .mt_3()
                    .h(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap_2()
                    .rounded(px(8.))
                    .bg(rgb(color::input()))
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(color::text()))
                    .cursor_pointer()
                    .hover(|d| d.bg(rgb(color::active())))
                    .child(icon("clock", 14., color::muted()))
                    .child(if self.loading_older {
                        "Chargement…"
                    } else {
                        "Charger les messages précédents"
                    })
                    .on_click(cx.listener(|this, _, _, cx| this.load_older(cx)))
            });
            let mut index = Vec::new();
            let rows = self.message_rows(&mut index, cx);
            // Children before the first row: [older] + welcome.
            let offset = usize::from(older.is_some()) + 1;
            if let Some(id) = self.jump_to.clone() {
                if let Some((_, ix)) = index.iter().find(|(m, _)| *m == id) {
                    self.scroll.scroll_to_item(ix + offset);
                    self.flash = Some((id, std::time::Instant::now()));
                    self.jump_to = None;
                }
            }
            let away = {
                let max = self.scroll.max_offset().height;
                max > px(0.) && self.scroll.offset().y < -max + px(160.)
            };
            let jump = away.then(|| {
                div()
                    .id("jump-bottom")
                    .absolute()
                    .bottom(px(12.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .child(
                        div()
                            .id("jump-pill")
                            .px_3()
                            .h(px(30.))
                            .rounded_full()
                            .bg(rgb(color::brand()))
                            .hover(|d| d.bg(rgb(color::brand_hover())))
                            .shadow_lg()
                            .cursor_pointer()
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xffffff))
                            .child("Aller aux messages récents")
                            .child(icon("arrow-down", 14., 0xffffff))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.scroll.scroll_to_bottom();
                                cx.notify();
                            })),
                    )
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
                        .child(div().h(px(20.)).flex_shrink_0()),
                )
                .children(jump)
                .into_any_element()
        } else if self.guild.is_none() {
            self.friends_view(cx)
        } else {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap_2()
                .text_color(rgb(color::muted()))
                .child(icon("hash", 40., color::muted()))
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

        let banner: Option<AnyElement> = if let Some(r) = &self.replying {
            Some(self.banner(
                "reply",
                format!("Réponse à {}", r.author.display_name()),
                cx,
            ))
        } else if self.editing.is_some() {
            Some(self.banner(
                "pencil",
                "Modification du message · Échap pour annuler".into(),
                cx,
            ))
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
                    .h(px(52.))
                    .px_4()
                    .rounded(px(10.))
                    .bg(rgb(color::input()))
                    .flex()
                    .items_center()
                    .gap_3()
                    .text_color(rgb(color::muted()))
                    .child(icon("lock", 18., color::muted()))
                    .child("Vous n'avez pas la permission d'envoyer des messages dans ce salon."),
            )
        });
        let picker_open = self.picker.is_some();
        let gif_open = picker_open && self.picker_tab == PickerTab::Gif;
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
                        .min_h(px(52.))
                        .py(px(14.))
                        .pl_3()
                        .pr_2()
                        .when(has_banner, |d| d.rounded_b(px(10.)))
                        .when(!has_banner, |d| d.rounded(px(10.)))
                        .bg(rgb(color::input()))
                        .border_1()
                        .border_color(rgb(color::border()))
                        .flex()
                        .items_start()
                        .gap_3()
                        .child(
                            div()
                                .id("upload")
                                .group("upload")
                                .mt(px(-2.))
                                .cursor_pointer()
                                .child(
                                    icon("circle-plus", 24., color::muted())
                                        .group_hover("upload", |s| {
                                            s.text_color(rgb(color::bright()))
                                        }),
                                )
                                .tooltip(tip("Envoyer un fichier"))
                                .on_click(cx.listener(|this, _, _, cx| this.pick_files(cx))),
                        )
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .child(self.input_line(placeholder, false)),
                        )
                        .child(
                            div()
                                .mt(px(-6.))
                                .flex()
                                .items_center()
                                .child(icon_button("gif-btn", "image", "GIF", gif_open).on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.picker = Some(Picker::Composer);
                                        this.open_gif_tab(cx);
                                    }),
                                ))
                                .child(
                                    icon_button(
                                        "emoji-btn",
                                        "smile",
                                        "Émojis",
                                        picker_open && !gif_open,
                                    )
                                    .on_click(cx.listener(
                                        |this, _, _, cx| {
                                            this.picker_tab = PickerTab::Emoji;
                                            this.gif_typing = false;
                                            this.picker = match this.picker {
                                                Some(_) => None,
                                                None => Some(Picker::Composer),
                                            };
                                            cx.notify();
                                        },
                                    )),
                                ),
                        ),
                )
                .child(
                    div()
                        .h(px(24.))
                        .flex()
                        .items_center()
                        .gap_1()
                        .text_xs()
                        .text_color(rgb(color::text()))
                        .when_some(self.typing_text(), |d, t| {
                            d.child(div().flex().gap(px(3.)).children((0..3).map(|i| {
                                div()
                                    .size(px(5.))
                                    .rounded_full()
                                    .bg(rgb(color::bright()))
                                    .with_animation(
                                        SharedString::from(format!("typing-{i}")),
                                        Animation::new(std::time::Duration::from_millis(
                                            900 + i * 150,
                                        ))
                                        .repeat()
                                        .with_easing(pulsating_between(0.25, 1.)),
                                        |d, v| d.opacity(v),
                                    )
                            })))
                            .child(div().font_weight(FontWeight::MEDIUM).child(t))
                        }),
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
                    div().px_4().pb_2().child(
                        div()
                            .px_3()
                            .py_2()
                            .rounded(px(8.))
                            .bg(rgba((color::red() << 8) | 0x26))
                            .flex()
                            .items_center()
                            .gap_2()
                            .text_sm()
                            .text_color(rgb(color::red()))
                            .child(icon("circle-alert", 16., color::red()))
                            .child(self.status.clone()),
                    ),
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
            .border_t_1()
            .border_l_1()
            .border_color(rgb(color::border()))
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
                            .top(px(48.))
                            .bottom_0()
                            .right_0()
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
        appear(
            "emoji-backdrop-anim",
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
                ),
        )
    }

    fn friends_view(&self, cx: &mut Context<Self>) -> AnyElement {
        let online = |u: &User| {
            matches!(
                self.presence.get(&u.id).map(|s| s.as_str()),
                Some("online") | Some("idle") | Some("dnd")
            )
        };
        let mut list: Vec<&User> = self
            .friends
            .iter()
            .filter(|u| self.friends_all || online(u))
            .collect();
        list.sort_by_key(|u| {
            let rank = match self.presence.get(&u.id).map(|s| s.as_str()) {
                Some("online") => 0,
                Some("idle") => 1,
                Some("dnd") => 2,
                _ => 3,
            };
            (rank, u.display_name().to_lowercase())
        });
        let n_online = self.friends.iter().filter(|u| online(u)).count();
        let tab = |id: &'static str, label: String, on: bool| {
            div()
                .id(id)
                .px_3()
                .h(px(28.))
                .rounded(px(8.))
                .flex()
                .items_center()
                .cursor_pointer()
                .text_sm()
                .font_weight(FontWeight::MEDIUM)
                .text_color(rgb(if on { color::bright() } else { color::muted() }))
                .when(on, |d| d.bg(rgb(color::active())))
                .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
                .child(label)
        };
        let header = div()
            .px_6()
            .pt_4()
            .pb_2()
            .flex()
            .items_center()
            .gap_2()
            .child(
                tab("ft-online", "En ligne".into(), !self.friends_all).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.friends_all = false;
                        cx.notify();
                    },
                )),
            )
            .child(
                tab("ft-all", "Tous".into(), self.friends_all).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.friends_all = true;
                        cx.notify();
                    },
                )),
            );
        let label = if self.friends_all {
            format!("TOUS LES AMIS — {}", self.friends.len())
        } else {
            format!("EN LIGNE — {n_online}")
        };
        let mut body = div()
            .id("friends")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px_6()
            .pb_4()
            .flex()
            .flex_col()
            .child(div().py_2().child(Self::section_label(label)));
        if list.is_empty() {
            body = body.child(
                div()
                    .py_12()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap_3()
                    .text_color(rgb(color::muted()))
                    .child(
                        div()
                            .size(px(96.))
                            .rounded_full()
                            .bg(rgb(color::input()))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon("users", 44., color::muted())),
                    )
                    .child(if self.friends_all {
                        "Aucun ami à afficher pour le moment."
                    } else {
                        "Personne n'est en ligne pour le moment."
                    }),
            );
        }
        for u in list {
            let st = self
                .presence
                .get(&u.id)
                .map(|s| s.as_str())
                .unwrap_or("offline");
            let (open, dm, prof) = (u.clone(), u.clone(), u.clone());
            let g: SharedString = format!("fr-{}", u.id).into();
            body = body.child(
                div()
                    .id(SharedString::from(format!("f-{}", u.id)))
                    .group(g.clone())
                    .h(px(62.))
                    .px_3()
                    .rounded(px(10.))
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
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap_2()
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
                                            .opacity(0.)
                                            .group_hover(g.clone(), |s| s.opacity(1.))
                                            .child(u.username.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .text_sm()
                                    .text_color(rgb(color::muted()))
                                    .child(status_label(st)),
                            ),
                    )
                    .child(
                        icon_button(
                            format!("fdm-{}", u.id),
                            "message-circle",
                            "Envoyer un message",
                            false,
                        )
                        .bg(rgb(color::frame()))
                        .rounded_full()
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.stop_propagation();
                            this.open_dm_with(&dm, cx)
                        })),
                    )
                    .child(
                        icon_button(format!("fpr-{}", u.id), "user", "Voir le profil", false)
                            .bg(rgb(color::frame()))
                            .rounded_full()
                            .on_click(cx.listener(move |this, _, _, cx| {
                                cx.stop_propagation();
                                this.open_profile(prof.clone(), cx)
                            })),
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.open_dm_with(&open, cx))),
            );
        }
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(body)
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

    fn banner(&self, glyph: &str, text: String, cx: &mut Context<Self>) -> AnyElement {
        div()
            .px_3()
            .py(px(8.))
            .rounded_t(px(10.))
            .bg(rgb(color::panel()))
            .border_1()
            .border_b_0()
            .border_color(rgb(color::border()))
            .flex()
            .items_center()
            .gap_2()
            .text_sm()
            .text_color(rgb(color::muted()))
            .child(icon(glyph, 14., color::muted()))
            .child(div().flex_1().overflow_hidden().child(text))
            .child(
                icon_button("cancel-compose", "x", "Annuler (Échap)", false).on_click(cx.listener(
                    |this, _, _, cx| {
                        this.cancel_compose();
                        cx.notify();
                    },
                )),
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
                this.open_profile(user.clone(), cx);
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
                        let tint = self.roles.get(id).map(|r| r.color).filter(|c| *c != 0);
                        panel = panel.child(
                            Self::section_label(format!("{name} — {count}"))
                                .when_some(tint, |d, c| d.text_color(rgb(c))),
                        );
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
        let text: String = self.preview_text(m).chars().take(240).collect();
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
                    .child(div().text_sm().child(text)),
            )
    }

    fn profile_overlay(&self, user: &User, cx: &mut Context<Self>) -> AnyElement {
        let data = self.profile_data.as_ref();
        let u = data.map(|d| &d.user).unwrap_or(user).clone();
        let is_me = self.me.as_ref().is_some_and(|m| m.id == u.id);
        let accent = data
            .and_then(|d| d.accent_color)
            .or(u.accent_color)
            .unwrap_or_else(color::brand);
        let banner: AnyElement = match u.banner_url().and_then(|b| self.images.get(&b).cloned()) {
            Some(i) => img(i)
                .w_full()
                .h(px(110.))
                .object_fit(ObjectFit::Cover)
                .into_any_element(),
            None => div()
                .w_full()
                .h(px(110.))
                .bg(rgb(accent))
                .into_any_element(),
        };

        // Display name: guild nickname first.
        let shown = data
            .and_then(|d| d.nick.clone())
            .unwrap_or_else(|| u.display_name().to_string());
        let tint = self.name_color(&u.id);
        let mut name_row = div().flex().items_center().gap_2().child(
            div()
                .text_size(px(22.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(tint.unwrap_or_else(color::bright)))
                .child(shown),
        );
        if u.bot {
            name_row = name_row.child(Self::chip("BOT", color::brand(), 0xffffff));
        }
        if let Some(tag) = u.tag() {
            name_row = name_row.child(Self::chip(tag, color::active(), color::bright()));
        }
        let mut sub = u.username.clone();
        if let Some(p) = data.and_then(|d| d.pronouns.clone()) {
            sub = format!("{sub} · {p}");
        }
        let status = self.presence.get(&u.id).map(|s| s.as_str());

        // Badges: server-provided icons, else text badges from the public flags.
        let mut badges = div().flex().flex_wrap().gap_1();
        let mut any_badge = false;
        for b in data.map(|d| d.badges.as_slice()).unwrap_or(&[]) {
            any_badge = true;
            badges = badges.child(match self.images.get(&b.url()) {
                Some(i) => img(i.clone()).size(px(22.)).into_any_element(),
                None => {
                    Self::chip(&b.description, color::active(), color::bright()).into_any_element()
                }
            });
        }
        if !any_badge {
            for (bit, label) in FLAG_BADGES {
                if u.public_flags & bit != 0 {
                    any_badge = true;
                    badges = badges.child(Self::chip(label, color::active(), color::bright()));
                }
            }
        }

        // Body sections.
        let mut body = div().flex().flex_col().gap_3();
        let section = |title: &str| {
            div()
                .text_xs()
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(color::bright()))
                .child(title.to_string())
        };
        if self.profile_loading && data.is_none() {
            body = body
                .child(skeleton("sk-pf-1", Some(220.), 14., Some(6.)))
                .child(skeleton("sk-pf-2", Some(300.), 14., Some(6.)))
                .child(skeleton("sk-pf-3", Some(160.), 14., Some(6.)));
        }
        if !self.profile_err.is_empty() {
            body = body.child(
                div()
                    .text_sm()
                    .text_color(rgb(color::muted()))
                    .child(format!(
                        "Profil complet indisponible : {}",
                        self.profile_err
                    )),
            );
        }
        if let Some(bio) = data.and_then(|d| d.bio.clone()) {
            let (text, hl) = markdown(&bio, false);
            body = body.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(section("À PROPOS DE MOI"))
                    .child(
                        div()
                            .text_sm()
                            .child(StyledText::new(text).with_highlights(hl)),
                    ),
            );
        }
        let mut dates = div().flex().gap_6();
        dates = dates.child(
            div()
                .flex()
                .flex_col()
                .gap_1()
                .child(section("MEMBRE DE DISCORD DEPUIS"))
                .child(div().text_sm().child(fmt_date_ms(u.created_ms()))),
        );
        if let Some(j) = data.and_then(|d| d.joined_at.clone()) {
            dates = dates.child(
                div()
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(section("MEMBRE DU SERVEUR DEPUIS"))
                    .child(div().text_sm().child(fmt_date_iso(&j))),
            );
        }
        body = body.child(dates);

        if let Some(d) = data {
            let mut roles: Vec<(&String, &api::Role)> = d
                .member_roles
                .iter()
                .filter_map(|id| self.roles.get(id).map(|r| (id, r)))
                .collect();
            roles.sort_by_key(|(_, r)| std::cmp::Reverse(r.position));
            if !roles.is_empty() {
                body =
                    body.child(
                        div()
                            .flex()
                            .flex_col()
                            .gap_1()
                            .child(section("RÔLES"))
                            .child(div().flex().flex_wrap().gap_1().children(
                                roles.into_iter().map(|(_, r)| {
                                    div()
                                        .px_2()
                                        .py(px(2.))
                                        .rounded(px(4.))
                                        .bg(rgb(color::active()))
                                        .flex()
                                        .items_center()
                                        .gap_1()
                                        .text_xs()
                                        .child(div().size(px(10.)).rounded_full().bg(rgb(
                                            if r.color != 0 {
                                                r.color
                                            } else {
                                                color::muted()
                                            },
                                        )))
                                        .child(r.name.clone())
                                }),
                            )),
                    );
            }
            if let Some(since) = &d.premium_since {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(section("NITRO"))
                        .child(
                            div()
                                .text_sm()
                                .child(format!("Abonné depuis le {}", fmt_date_iso(since))),
                        ),
                );
            }
            if let Some(since) = &d.premium_guild_since {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(section("BOOSTE UN SERVEUR"))
                        .child(
                            div()
                                .text_sm()
                                .child(format!("Depuis le {}", fmt_date_iso(since))),
                        ),
                );
            }
            if !d.mutual_guilds.is_empty() || d.mutual_friends_count > 0 {
                let names: Vec<String> = d
                    .mutual_guilds
                    .iter()
                    .map(|(id, nick)| {
                        let n = self
                            .guilds
                            .iter()
                            .find(|g| &g.id == id)
                            .map(|g| g.name.clone())
                            .unwrap_or_else(|| "Serveur".into());
                        match nick {
                            Some(nick) => format!("{n} ({nick})"),
                            None => n,
                        }
                    })
                    .collect();
                let more = names.len().saturating_sub(8);
                let mut block = div().flex().flex_col().gap_1();
                if !names.is_empty() {
                    block = block
                        .child(section(&format!("SERVEURS EN COMMUN — {}", names.len())))
                        .child(
                            div().flex().flex_wrap().gap_1().children(
                                names
                                    .into_iter()
                                    .take(8)
                                    .map(|n| Self::chip(&n, color::active(), color::bright())),
                            ),
                        );
                    if more > 0 {
                        block = block.child(
                            div()
                                .text_xs()
                                .text_color(rgb(color::muted()))
                                .child(format!("+ {more} autres")),
                        );
                    }
                }
                if d.mutual_friends_count > 0 {
                    block = block.child(div().text_sm().child(format!(
                        "{} ami{} en commun",
                        d.mutual_friends_count,
                        if d.mutual_friends_count > 1 { "s" } else { "" }
                    )));
                }
                body = body.child(block);
            }
            if !d.connections.is_empty() {
                body = body.child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_1()
                        .child(section("CONNEXIONS"))
                        .children(d.connections.iter().map(|c| {
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .child(
                                    div()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .text_color(rgb(color::bright()))
                                        .child(connection_label(&c.kind)),
                                )
                                .child(div().text_color(rgb(color::muted())).child(c.name.clone()))
                                .when(c.verified, |d| {
                                    d.child(div().text_color(rgb(color::green())).child("✓"))
                                })
                        })),
                );
            }
        }

        let (u2, id) = (u.clone(), u.id.clone());
        let button = |label: &'static str, id: &'static str, primary: bool| {
            div()
                .id(id)
                .h(px(34.))
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
        let max_h = (self.height - 48.).max(240.);
        appear(
            "profile-backdrop-anim",
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
                    this.profile_data = None;
                    cx.notify();
                }))
                .child(
                    div()
                        .id("profile")
                        .w(px(420.0_f32.min(self.width - 32.)))
                        .max_h(px(max_h))
                        .rounded(px(8.))
                        .bg(rgb(color::sidebar()))
                        .shadow_lg()
                        .overflow_y_scroll()
                        .on_click(|_, _, cx| cx.stop_propagation())
                        .child(banner)
                        .child(
                            div()
                                .px_4()
                                .pb_4()
                                .flex()
                                .flex_col()
                                .gap_3()
                                .child(
                                    div()
                                        .mt(px(-44.))
                                        .size(px(96.))
                                        .rounded_full()
                                        .bg(rgb(color::sidebar()))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(self.avatar_status(&u, 84., color::sidebar())),
                                )
                                .child(
                                    div()
                                        .flex()
                                        .flex_col()
                                        .child(name_row)
                                        .child(div().text_color(rgb(color::muted())).child(sub))
                                        .children(status.map(|s| {
                                            div()
                                                .text_xs()
                                                .text_color(rgb(color::muted()))
                                                .child(status_label(s))
                                        })),
                                )
                                .when(any_badge, |d| d.child(badges))
                                .child(
                                    div()
                                        .flex()
                                        .gap_2()
                                        .when(!is_me, |d| {
                                            d.child(
                                                button("Envoyer un message", "profile-dm", true)
                                                    .on_click(cx.listener(
                                                        move |this, _, _, cx| {
                                                            this.profile = None;
                                                            if this.guild.is_some() {
                                                                this.guild = None;
                                                                this.channels.clear();
                                                            }
                                                            this.open_dm_with(&u2, cx);
                                                        },
                                                    )),
                                            )
                                        })
                                        .child(
                                            button("Copier l'ID", "profile-copy", false).on_click(
                                                cx.listener(move |_, _, _, cx| {
                                                    cx.write_to_clipboard(
                                                        gpui::ClipboardItem::new_string(id.clone()),
                                                    )
                                                }),
                                            ),
                                        ),
                                )
                                .child(
                                    div()
                                        .p_3()
                                        .rounded(px(8.))
                                        .bg(rgb(color::chat()))
                                        .child(body),
                                ),
                        ),
                ),
        )
    }

    fn chip(text: &str, bg: u32, fg: u32) -> Div {
        div()
            .px_2()
            .py(px(1.))
            .rounded(px(4.))
            .bg(rgb(bg))
            .text_size(px(11.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(fg))
            .child(text.to_string())
    }

    fn settings_overlay(&self, cx: &mut Context<Self>) -> AnyElement {
        let me = self.me.clone().unwrap_or_default();
        let tabs: [(&str, &str); 4] = [
            ("user", "Mon compte"),
            ("palette", "Apparence"),
            ("bell", "Notifications"),
            ("sliders-horizontal", "Avancé"),
        ];
        let toggle = |id: &'static str, label: &'static str, help: &'static str, on: bool| {
            div()
                .id(id)
                .py_3()
                .flex()
                .items_center()
                .gap_4()
                .cursor_pointer()
                .border_b_1()
                .border_color(rgb(color::border()))
                .child(
                    div()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(color::bright()))
                                .child(label),
                        )
                        .child(div().text_sm().text_color(rgb(color::muted())).child(help)),
                )
                .child(
                    div()
                        .w(px(40.))
                        .h(px(24.))
                        .flex_shrink_0()
                        .rounded_full()
                        .bg(rgb(if on { color::brand() } else { color::active() }))
                        .flex()
                        .items_center()
                        .when(on, |d| d.justify_end())
                        .px(px(3.))
                        .child(
                            div()
                                .size(px(18.))
                                .rounded_full()
                                .bg(rgb(0xffffff))
                                .shadow_sm()
                                .flex()
                                .items_center()
                                .justify_center()
                                .child(icon(
                                    if on { "check" } else { "x" },
                                    12.,
                                    if on { color::brand() } else { color::muted() },
                                )),
                        ),
                )
        };
        let heading = |t: &str| {
            div()
                .mb_3()
                .text_size(px(20.))
                .font_weight(FontWeight::BOLD)
                .text_color(rgb(color::bright()))
                .child(t.to_string())
        };

        let content: AnyElement = match self.settings_tab {
            1 => {
                let cards = color::Theme::ALL.into_iter().map(|t| {
                    let selected = self.theme == t;
                    // Preview colours of that theme.
                    let prev = color::theme();
                    color::set_theme(t);
                    let (frame, side, chat, text, bright) =
                        (color::frame(), color::sidebar(), color::chat(), color::muted(), color::bright());
                    color::set_theme(prev);
                    div()
                        .id(SharedString::from(format!("theme-{}", t.key())))
                        .w(px(132.))
                        .flex()
                        .flex_col()
                        .gap_2()
                        .cursor_pointer()
                        .child(
                            div()
                                .h(px(84.))
                                .rounded(px(10.))
                                .overflow_hidden()
                                .border_2()
                                .border_color(rgb(if selected { color::brand() } else { color::border() }))
                                .bg(rgb(frame))
                                .flex()
                                .p(px(6.))
                                .gap(px(4.))
                                .child(
                                    div()
                                        .w(px(14.))
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.))
                                        .children((0..3).map(|_| div().size(px(12.)).rounded(px(4.)).bg(rgb(side)))),
                                )
                                .child(div().w(px(30.)).rounded(px(4.)).bg(rgb(side)))
                                .child(
                                    div()
                                        .flex_1()
                                        .rounded(px(4.))
                                        .bg(rgb(chat))
                                        .p(px(4.))
                                        .flex()
                                        .flex_col()
                                        .gap(px(4.))
                                        .child(div().w(px(36.)).h(px(4.)).rounded_full().bg(rgb(bright)))
                                        .child(div().w(px(48.)).h(px(4.)).rounded_full().bg(rgb(text)))
                                        .child(div().w(px(28.)).h(px(4.)).rounded_full().bg(rgb(text))),
                                ),
                        )
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .gap_2()
                                .text_sm()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(if selected { color::bright() } else { color::muted() }))
                                .when(selected, |d| d.child(icon("check", 14., color::brand())))
                                .child(t.label()),
                        )
                        .on_click(cx.listener(move |this, _, _, cx| this.set_theme(t, cx)))
                });
                let dens = [("compact", "Compacte"), ("default", "Par défaut"), ("spacious", "Spacieuse")];
                div()
                    .flex()
                    .flex_col()
                    .gap_2()
                    .child(heading("Apparence"))
                    .child(Self::section_label("THÈME".into()))
                    .child(div().flex().flex_wrap().gap_4().children(cards))
                    .child(div().h(px(12.)))
                    .child(Self::section_label("DENSITÉ DE L'INTERFACE".into()))
                    .child(
                        div()
                            .p_1()
                            .rounded(px(10.))
                            .bg(rgb(color::frame()))
                            .flex()
                            .gap_1()
                            .children(dens.into_iter().map(|(k, label)| {
                                let on = self.density == k;
                                div()
                                    .id(SharedString::from(format!("dens-{k}")))
                                    .flex_1()
                                    .h(px(32.))
                                    .rounded(px(8.))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .cursor_pointer()
                                    .text_sm()
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(rgb(if on { color::bright() } else { color::muted() }))
                                    .when(on, |d| d.bg(rgb(color::active())).shadow_sm())
                                    .hover(|d| d.text_color(rgb(color::bright())))
                                    .child(label)
                                    .on_click(cx.listener(move |this, _, _, cx| this.set_density(k, cx)))
                            })),
                    )
                    .into_any_element()
            }
            2 => div()
                .flex()
                .flex_col()
                .child(heading("Notifications"))
                .child(
                    toggle(
                        "set-notif",
                        "Notifications du bureau",
                        "Pour les mentions et les messages privés quand la fenêtre n'est pas active.",
                        self.notifications,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.notifications = !this.notifications;
                        this.save_prefs();
                        cx.notify();
                    })),
                )
                .child(
                    toggle(
                        "set-title",
                        "Compteur dans le titre de la fenêtre",
                        "Affiche « (3) Discord » quand vous avez des mentions non lues.",
                        self.title_badge,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.title_badge = !this.title_badge;
                        this.save_prefs();
                        cx.notify();
                    })),
                )
                .into_any_element(),
            3 => div()
                .flex()
                .flex_col()
                .child(heading("Avancé"))
                .child(
                    toggle(
                        "set-side",
                        "Panneau latéral",
                        "Liste des membres, recherche et épingles à droite du chat.",
                        self.show_side,
                    )
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.show_side = !this.show_side;
                        this.save_prefs();
                        cx.notify();
                    })),
                )
                .child(
                    div()
                        .mt_4()
                        .text_sm()
                        .text_color(rgb(color::muted()))
                        .child("Icônes Lucide (ISC) · Police Inter (OFL) · Interface GPUI"),
                )
                .into_any_element(),
            _ => {
                let banner: AnyElement = match me.banner_url().and_then(|b| self.images.get(&b).cloned()) {
                    Some(i) => img(i).w_full().h(px(100.)).object_fit(ObjectFit::Cover).into_any_element(),
                    None => div()
                        .w_full()
                        .h(px(100.))
                        .bg(linear_gradient(
                            90.,
                            linear_color_stop(rgb(color::brand()), 0.),
                            linear_color_stop(rgb(0xeb459e), 1.),
                        ))
                        .into_any_element(),
                };
                div()
                    .flex()
                    .flex_col()
                    .gap_3()
                    .child(heading("Mon compte"))
                    .child(
                        div()
                            .rounded(px(12.))
                            .overflow_hidden()
                            .bg(rgb(color::frame()))
                            .child(banner)
                            .child(
                                div()
                                    .px_4()
                                    .pb_4()
                                    .flex()
                                    .items_end()
                                    .gap_3()
                                    .child(
                                        div()
                                            .mt(px(-30.))
                                            .p(px(5.))
                                            .rounded_full()
                                            .bg(rgb(color::frame()))
                                            .child(self.avatar(&me, 72.)),
                                    )
                                    .child(
                                        div()
                                            .flex_1()
                                            .flex()
                                            .flex_col()
                                            .child(
                                                div()
                                                    .text_size(px(18.))
                                                    .font_weight(FontWeight::BOLD)
                                                    .text_color(rgb(color::bright()))
                                                    .child(me.display_name().to_string()),
                                            )
                                            .child(div().text_sm().text_color(rgb(color::muted())).child(me.username.clone())),
                                    )
                                    .child(
                                        div()
                                            .id("my-profile")
                                            .px_3()
                                            .h(px(32.))
                                            .rounded(px(8.))
                                            .bg(rgb(color::brand()))
                                            .hover(|d| d.bg(rgb(color::brand_hover())))
                                            .flex()
                                            .items_center()
                                            .cursor_pointer()
                                            .text_sm()
                                            .font_weight(FontWeight::MEDIUM)
                                            .text_color(rgb(0xffffff))
                                            .child("Voir le profil")
                                            .on_click({
                                                let me = me.clone();
                                                cx.listener(move |this, _, _, cx| {
                                                    this.settings = false;
                                                    this.open_profile(me.clone(), cx);
                                                })
                                            }),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .id("set-logout")
                            .mt_4()
                            .w(px(160.))
                            .h(px(36.))
                            .rounded(px(8.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .gap_2()
                            .cursor_pointer()
                            .bg(rgb(color::red()))
                            .hover(|d| d.opacity(0.9))
                            .text_sm()
                            .font_weight(FontWeight::MEDIUM)
                            .text_color(rgb(0xffffff))
                            .child(icon("log-out", 16., 0xffffff))
                            .child("Se déconnecter")
                            .on_click(cx.listener(|this, _, _, cx| this.logout(cx))),
                    )
                    .into_any_element()
            }
        };

        let nav = div()
            .w(px(200.))
            .flex_shrink_0()
            .p_3()
            .bg(rgb(color::sidebar()))
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(Self::section_label("PARAMÈTRES".into()))
            .children(tabs.into_iter().enumerate().map(|(i, (glyph, label))| {
                let on = self.settings_tab as usize == i;
                div()
                    .id(SharedString::from(format!("stab-{i}")))
                    .h(px(34.))
                    .px_2()
                    .rounded(px(8.))
                    .flex()
                    .items_center()
                    .gap_2()
                    .cursor_pointer()
                    .text_sm()
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(rgb(if on { color::bright() } else { color::muted() }))
                    .when(on, |d| d.bg(rgb(color::active())))
                    .hover(|d| d.bg(rgb(color::hover())).text_color(rgb(color::bright())))
                    .child(icon(
                        glyph,
                        16.,
                        if on { color::bright() } else { color::muted() },
                    ))
                    .child(label)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.settings_tab = i as u8;
                        cx.notify();
                    }))
            }));

        let wide = self.width >= 700.;
        let panel = div()
            .id("settings")
            .w(px(780.0_f32.min(self.width - 32.)))
            .h(px((self.height - 64.).clamp(320., 560.)))
            .rounded(px(14.))
            .overflow_hidden()
            .bg(rgb(color::chat()))
            .border_1()
            .border_color(rgb(color::border()))
            .shadow_lg()
            .flex()
            .on_click(|_, _, cx| cx.stop_propagation())
            .when(wide, |d| d.child(nav))
            .child(
                div()
                    .id("settings-body")
                    .flex_1()
                    .min_w_0()
                    .relative()
                    .overflow_y_scroll()
                    .p_6()
                    .child(content)
                    .child(div().absolute().top(px(12.)).right(px(12.)).child(
                        icon_button("set-close", "x", "Fermer (Échap)", false).on_click(
                            cx.listener(|this, _, _, cx| {
                                this.settings = false;
                                cx.notify();
                            }),
                        ),
                    )),
            );
        div()
            .id("settings-backdrop")
            .absolute()
            .top_0()
            .left_0()
            .size_full()
            .bg(rgba(0x000000b0))
            .flex()
            .items_center()
            .justify_center()
            .on_click(cx.listener(|this, _, _, cx| {
                this.settings = false;
                cx.notify();
            }))
            .child(appear("settings-anim", panel))
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

    fn message_rows(
        &self,
        index: &mut Vec<(String, usize)>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let mut rows = Vec::with_capacity(self.messages.len());
        let mut prev: Option<(&Message, DateTime<Local>)> = None;
        for m in &self.messages {
            let time = parse_time(&m.timestamp);
            let new_day = match (&prev, &time) {
                (Some((_, p)), Some(t)) => p.date_naive() != t.date_naive(),
                (None, Some(_)) => true,
                _ => false,
            };
            let is_new = self.new_since.as_deref() == Some(&m.id);
            if new_day {
                if let Some(t) = &time {
                    rows.push(Self::day_divider(t, is_new));
                }
            } else if is_new {
                rows.push(Self::new_divider());
            }
            let grouped = !new_day
                && !is_new
                && m.kind != 19
                && !matches!(m.kind, 1..=18 | 21)
                && match (&prev, &time) {
                    (Some((p, pt)), Some(t)) => {
                        p.author.id == m.author.id && (*t - *pt).num_minutes() < GROUP_MINUTES
                    }
                    _ => false,
                };
            index.push((m.id.clone(), rows.len()));
            rows.push(self.message_row(m, time.as_ref(), grouped, cx));
            if let Some(t) = time {
                prev = Some((m, t));
            }
        }
        rows
    }

    /// Red "NOUVEAUX" line before the first unread message.
    fn new_divider() -> AnyElement {
        div()
            .mx_4()
            .my_2()
            .flex()
            .items_center()
            .child(div().flex_1().h(px(1.)).bg(rgb(color::red())))
            .child(
                div()
                    .px_1()
                    .rounded(px(4.))
                    .bg(rgb(color::red()))
                    .text_size(px(10.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgb(0xffffff))
                    .child("NOUVEAUX"),
            )
            .into_any_element()
    }

    fn day_divider(t: &DateTime<Local>, new: bool) -> AnyElement {
        let line_color = if new { color::red() } else { color::divider() };
        let line = move || div().flex_1().h(px(1.)).bg(rgb(line_color));
        div()
            .mx_4()
            .my_3()
            .flex()
            .items_center()
            .gap_2()
            .text_xs()
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(if new { color::red() } else { color::muted() }))
            .child(line())
            .child(format!(
                "{} {} {}",
                t.day(),
                MONTHS[t.month0() as usize],
                t.year()
            ))
            .child(line())
            .when(new, |d| {
                d.child(
                    div()
                        .px_1()
                        .rounded(px(4.))
                        .bg(rgb(color::red()))
                        .text_size(px(10.))
                        .font_weight(FontWeight::BOLD)
                        .text_color(rgb(0xffffff))
                        .child("NOUVEAUX"),
                )
            })
            .into_any_element()
    }

    /// Message text with mentions resolved and Markdown applied.
    fn rich_text(&self, m: &Message, edited: bool, cx: &mut Context<Self>) -> AnyElement {
        let lookup = |kind: char, id: &str| -> Option<String> {
            match kind {
                'u' => m
                    .mentions
                    .iter()
                    .find(|u| u.id == id)
                    .map(|u| u.display_name().to_string())
                    .or_else(|| self.known_users.get(id).cloned())
                    .map(|n| format!("@{n}")),
                'r' => self.roles.get(id).map(|r| format!("@{}", r.name)),
                'c' => self
                    .channels
                    .iter()
                    .chain(&self.threads)
                    .chain(&self.dms)
                    .find(|c| c.id == id)
                    .map(|c| format!("#{}", c.title())),
                't' => {
                    let (secs, style) = id.split_once(':').unwrap_or((id, "f"));
                    secs.parse::<i64>().ok().map(|s| fmt_timestamp(s, style))
                }
                _ => None,
            }
        };
        let resolved = resolve_rich(&m.content, &lookup);
        let (text0, hl0) = markdown(&resolved, false);
        let (mut text, hl, spans) = extract_mentions(&text0, hl0);

        // Mention chips win over Markdown styling on the same characters.
        let mut styled: Vec<(Range<usize>, HighlightStyle)> = hl
            .into_iter()
            .filter(|(r, _)| !spans.iter().any(|s| overlaps(&s.range, r)))
            .collect();
        for s in &spans {
            let role_color = (s.kind == 'r')
                .then(|| self.roles.get(&s.id).map(|r| r.color).filter(|c| *c != 0))
                .flatten();
            let style = match (s.kind, role_color) {
                (_, Some(c)) => HighlightStyle {
                    color: Some(rgb(c).into()),
                    background_color: Some(rgba((c << 8) | 0x33).into()),
                    ..Default::default()
                },
                ('t', _) => HighlightStyle {
                    background_color: Some(rgba(0x80808040).into()),
                    ..Default::default()
                },
                _ => HighlightStyle {
                    color: Some(rgb(color::mention_fg()).into()),
                    background_color: Some(rgba(0x5865f24d).into()),
                    font_weight: Some(FontWeight::MEDIUM),
                    ..Default::default()
                },
            };
            styled.push((s.range.clone(), style));
        }
        if edited {
            let start = text.len();
            text.push_str(" (modifié)");
            styled.push((
                start..text.len(),
                HighlightStyle {
                    color: Some(rgb(color::muted()).into()),
                    ..Default::default()
                },
            ));
        }
        styled.sort_by_key(|(r, _)| r.start);

        // Clickable ranges: user / channel mentions, then plain links.
        let mut ranges: Vec<Range<usize>> = Vec::new();
        let mut actions: Vec<Action> = Vec::new();
        for s in &spans {
            match s.kind {
                'u' => {
                    ranges.push(s.range.clone());
                    actions.push(Action::User(s.id.clone()));
                }
                'c' => {
                    ranges.push(s.range.clone());
                    actions.push(Action::Channel(s.id.clone()));
                }
                _ => {}
            }
        }
        for (r, url) in link_ranges(&text) {
            ranges.push(r);
            actions.push(Action::Url(url));
        }
        let weak = cx.entity().downgrade();
        InteractiveText::new(
            SharedString::from(format!("t-{}", m.id)),
            StyledText::new(text).with_highlights(styled),
        )
        .on_click(ranges, move |ix, _, app| match &actions[ix] {
            Action::Url(u) => app.open_url(u),
            Action::User(id) => {
                let id = id.clone();
                weak.update(app, |this, cx| this.open_profile_id(&id, cx))
                    .ok();
            }
            Action::Channel(id) => {
                let id = id.clone();
                weak.update(app, |this, cx| this.open_channel_id(&id, cx))
                    .ok();
            }
        })
        .into_any_element()
    }

    /// Single-line preview of a message for reply headers.
    fn preview_text(&self, m: &Message) -> String {
        let lookup = |kind: char, id: &str| -> Option<String> {
            match kind {
                'u' => m
                    .mentions
                    .iter()
                    .find(|u| u.id == id)
                    .map(|u| u.display_name().to_string())
                    .or_else(|| self.known_users.get(id).cloned())
                    .map(|n| format!("@{n}")),
                'r' => self.roles.get(id).map(|r| format!("@{}", r.name)),
                'c' => self
                    .channels
                    .iter()
                    .find(|c| c.id == id)
                    .map(|c| format!("#{}", c.title())),
                _ => None,
            }
        };
        let (text, _) = markdown(
            &resolve_tokens(&m.content, &|k, i| {
                lookup(
                    match k {
                        '@' => 'u',
                        '&' => 'r',
                        other => other,
                    },
                    i,
                )
            }),
            false,
        );
        let text = text.replace('\n', " ");
        if text.trim().is_empty() {
            if m.attachments.is_empty() && m.embeds.is_empty() {
                "(message vide)".into()
            } else {
                "Cliquez pour voir la pièce jointe".into()
            }
        } else {
            text
        }
    }

    fn reply_header(&self, m: &Message, cx: &mut Context<Self>) -> Option<AnyElement> {
        if m.kind != 19 && m.referenced_message.is_none() {
            return None;
        }
        let connector = div()
            .w(px(24.))
            .h(px(10.))
            .mt(px(6.))
            .flex_shrink_0()
            .border_l_2()
            .border_t_2()
            .border_color(rgb(color::divider()))
            .rounded_tl(px(6.));
        let body: AnyElement =
            match &m.referenced_message {
                Some(r) => {
                    let author = r.author.clone();
                    let tint = self.name_color(&r.author.id).unwrap_or_else(color::muted);
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .min_w_0()
                        .flex_1()
                        .child(self.avatar(&r.author, 16.))
                        .child(
                            div()
                                .id(SharedString::from(format!("rp-{}", m.id)))
                                .flex_shrink_0()
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(rgb(tint))
                                .cursor_pointer()
                                .hover(|d| d.underline())
                                .child(format!("@{}", r.author.display_name()))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.open_profile(author.clone(), cx)
                                })),
                        )
                        .child({
                            let target = r.id.clone();
                            div()
                                .id(SharedString::from(format!("rpj-{}", m.id)))
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .cursor_pointer()
                                .hover(|d| d.text_color(rgb(color::bright())))
                                .child(self.preview_text(r))
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.jump_to_message(&target, cx)
                                }))
                        })
                        .into_any_element()
                }
                None => div()
                    .italic()
                    .child("Le message d'origine a été supprimé.")
                    .into_any_element(),
            };
        Some(
            div()
                .ml(px(20.))
                .mb(px(2.))
                .flex()
                .items_start()
                .gap_2()
                .text_sm()
                .text_color(rgb(color::muted()))
                .overflow_hidden()
                .child(connector)
                .child(body)
                .into_any_element(),
        )
    }

    /// Join / pin / boost / thread-created lines.
    fn system_row(&self, m: &Message, time: Option<&DateTime<Local>>) -> AnyElement {
        let who = m.author.display_name().to_string();
        let (icon, text) = match m.kind {
            1 => ("→", format!("{who} a ajouté quelqu'un au groupe.")),
            2 => ("←", format!("{who} a retiré quelqu'un du groupe.")),
            3 => ("☎", format!("{who} a démarré un appel.")),
            4 => (
                "✎",
                format!("{who} a changé le nom du groupe : {}", m.content),
            ),
            5 => ("✎", format!("{who} a changé l'icône du groupe.")),
            6 => ("📌", format!("{who} a épinglé un message à ce salon.")),
            7 => ("→", format!("{who} a rejoint le serveur.")),
            8..=11 => ("✦", format!("{who} a boosté le serveur !")),
            18 => ("⤷", format!("{who} a créé un fil : {}", m.content)),
            21 => ("⤷", format!("Message de départ du fil par {who}.")),
            _ => ("•", format!("Message système de {who}.")),
        };
        let accent = match m.kind {
            7 | 1 => color::green(),
            8..=11 => 0xf47fff,
            _ => color::muted(),
        };
        div()
            .px_4()
            .py(px(4.))
            .mt(px(8.))
            .flex()
            .items_center()
            .gap_3()
            .text_color(rgb(color::muted()))
            .child(
                div()
                    .w(px(40.))
                    .flex()
                    .justify_end()
                    .text_color(rgb(accent))
                    .child(icon),
            )
            .child(div().flex_1().min_w_0().child(text))
            .children(time.map(|t| div().text_xs().child(stamp(t))))
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

    /// Hover toolbar: react, reply, thread, copy, and edit/delete on our own messages.
    fn toolbar(&self, m: &Message, group: SharedString, cx: &mut Context<Self>) -> AnyElement {
        let mine = self.me.as_ref().is_some_and(|u| u.id == m.author.id);
        let armed = self.confirm_delete.as_deref() == Some(&m.id);
        let (reply, edit, del, react, thread) =
            (m.clone(), m.clone(), m.clone(), m.clone(), m.clone());
        let can_thread = m.thread.is_none()
            && self
                .channel
                .as_ref()
                .is_some_and(|c| !c.is_dm() && !c.is_thread());
        let copy = m.content.clone();
        let small = |id: String, name: &str, label: &str| {
            let g: SharedString = format!("tb-{id}").into();
            div()
                .id(SharedString::from(id))
                .group(g.clone())
                .size(px(30.))
                .rounded(px(6.))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|d| d.bg(rgb(color::hover())))
                .child(
                    icon(name, 18., color::muted())
                        .group_hover(g, |s| s.text_color(rgb(color::bright()))),
                )
                .tooltip(tip(label.to_string()))
        };
        div()
            .absolute()
            .top(px(-16.))
            .right(px(16.))
            .p(px(2.))
            .flex()
            .rounded(px(8.))
            .border_1()
            .border_color(rgb(color::border()))
            .bg(rgb(color::panel()))
            .shadow_md()
            .opacity(0.)
            .group_hover(group, |s| s.opacity(1.))
            .child(
                small(
                    format!("react-{}", m.id),
                    "smile-plus",
                    "Ajouter une réaction",
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.picker_tab = PickerTab::Emoji;
                    this.picker = Some(Picker::React(react.clone()));
                    cx.notify();
                })),
            )
            .child(
                small(format!("reply-{}", m.id), "reply", "Répondre")
                    .on_click(cx.listener(move |this, _, _, cx| this.start_reply(&reply, cx))),
            )
            .when(can_thread, |d| {
                d.child(
                    small(
                        format!("thread-{}", m.id),
                        "message-square-text",
                        "Créer un fil",
                    )
                    .on_click(cx.listener(move |this, _, _, cx| this.start_thread(&thread, cx))),
                )
            })
            .child(
                small(format!("copy-{}", m.id), "copy", "Copier le texte").on_click(cx.listener(
                    move |_, _, _, cx| {
                        cx.write_to_clipboard(gpui::ClipboardItem::new_string(copy.clone()))
                    },
                )),
            )
            .when(mine, |d| {
                d.child(
                    small(format!("edit-{}", m.id), "pencil", "Modifier")
                        .on_click(cx.listener(move |this, _, _, cx| this.start_edit(&edit, cx))),
                )
                .child(
                    small(
                        format!("del-{}", m.id),
                        "trash",
                        if armed {
                            "Cliquez à nouveau pour supprimer"
                        } else {
                            "Supprimer"
                        },
                    )
                    .when(armed, |d| d.bg(rgba((color::red() << 8) | 0x33)))
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
        if matches!(m.kind, 1..=18 | 21) && m.kind != 19 {
            return self.system_row(m, time);
        }
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
            if m.author.bot {
                head = head.child(Self::chip("BOT", color::brand(), 0xffffff));
            }
            if let Some(tag) = m.author.tag() {
                head = head.child(Self::chip(tag, color::active(), color::bright()));
            }
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
            text = text.child(self.rich_text(m, m.edited_timestamp.is_some(), cx));
        }
        for a in &m.attachments {
            text = text.child(div().mt_1().child(self.attachment(a)));
        }
        for s in &m.sticker_items {
            let el: AnyElement = match s.url() {
                Some(url) => match self.images.get(&url) {
                    Some(i) => img(i.clone())
                        .size(px(120.))
                        .object_fit(ObjectFit::Contain)
                        .into_any_element(),
                    None if self.image_failed.contains(&url) => div()
                        .text_color(rgb(color::muted()))
                        .child(format!("[Sticker : {}]", s.name))
                        .into_any_element(),
                    None => skeleton(format!("sk-st-{}", s.id), Some(120.), 120., Some(8.)),
                },
                None => div()
                    .text_color(rgb(color::muted()))
                    .child(format!("[Sticker : {}]", s.name))
                    .into_any_element(),
            };
            text = text.child(div().mt_1().child(el));
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
                    this.open_profile(a.clone(), cx);
                }))
                .into_any_element()
        };

        let pinged = m.mention_everyone
            || self
                .me
                .as_ref()
                .is_some_and(|me| m.mentions.iter().any(|u| u.id == me.id));
        let (gap, pad) = match self.density.as_str() {
            "compact" => (6., 1.),
            "spacious" => (22., 4.),
            _ => (16., 2.),
        };
        let flashing = self
            .flash
            .as_ref()
            .is_some_and(|(id, t)| *id == m.id && t.elapsed().as_secs_f32() < 2.5);
        let mut row = div()
            .group(group.clone())
            .relative()
            .when(pinged, |d| {
                d.bg(rgb(color::ping_bg()))
                    .border_l_2()
                    .border_color(rgb(color::yellow()))
            })
            .when(flashing, |d| d.bg(rgba(0x5865f22e)))
            .px_4()
            .py(px(pad))
            .when(!grouped, |d| d.mt(px(gap)))
            .hover(|d| d.bg(rgb(color::msg_hover())))
            .flex()
            .flex_col();

        if let Some(header) = self.reply_header(m, cx) {
            row = row.child(header);
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
