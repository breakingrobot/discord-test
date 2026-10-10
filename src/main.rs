mod api;
mod assets;
mod demo;
mod emoji;
mod gateway;
mod notify;
mod prefs;
mod qr;
mod store;
mod ui;
mod voice;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use gpui::{
    prelude::*, px, size, App, Application, Bounds, ClipboardItem, Context, FocusHandle, Focusable,
    Image, ImageFormat, KeyDownEvent, PathPromptOptions, Render, ScrollHandle, Window,
    WindowBounds, WindowOptions,
};

use api::{Channel, Emoji, Guild, GuildEmoji, Message, User};

/// REST polling happens only while the gateway is down.
const POLL_FAST: Duration = Duration::from_secs(4);
const TYPING_TTL: Duration = Duration::from_secs(8);
const MAX_INFLIGHT_IMAGES: usize = 16;
/// Decoded images kept around beyond what is on screen.
const MAX_CACHED_IMAGES: usize = 200;
/// Older history kept in memory per channel.
const MAX_MESSAGES: usize = 600;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Field {
    Email = 0,
    Password = 1,
    Token = 2,
    Code = 3,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LoginMode {
    Credentials,
    Token,
    Mfa,
}

pub enum QrState {
    Loading,
    Ready,
    Scanned(String),
    Failed(String),
}

#[derive(Clone, Default)]
pub struct Unread {
    pub guild: Option<String>,
    pub mentions: u32,
    /// First message received while away (for the "new messages" divider).
    pub first_id: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Members,
    Search,
    Pins,
}

/// One row of the slash-command autocomplete.
#[derive(Clone)]
pub struct SlashEntry {
    pub name: String,
    pub desc: String,
    pub app: String,
}

/// Commands handled locally by the client (they just rewrite the message text).
const BUILTIN_COMMANDS: &[(&str, &str)] = &[
    ("shrug", "Ajoute ¯\\_(ツ)_/¯ à votre message"),
    ("tableflip", "Ajoute (╯°□°)╯︵ ┻━┻ à votre message"),
    ("unflip", "Ajoute ┬─┬ ノ( ゜-゜ノ) à votre message"),
    ("me", "Met votre message en italique"),
    ("spoiler", "Cache votre message derrière un spoiler"),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum PickerTab {
    Emoji,
    Gif,
}

pub enum Picker {
    Composer,
    React(Message),
}

#[derive(Clone)]
pub enum Target {
    Home,
    Guild(String),
    Channel(Channel),
}

pub struct DiscordApp {
    focus: FocusHandle,
    scroll: ScrollHandle,
    /// `Authorization` header value (user token, or `Bot …`). Empty until login succeeds.
    auth: String,
    me: Option<User>,
    logged_in: bool,
    connected: bool,
    gateway_stop: Arc<AtomicBool>,
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
    show_side: bool,
    side: Side,
    /// Query being typed in the search box (`Some` = search input has focus).
    search: Option<String>,
    search_results: Vec<Message>,
    search_note: String,
    pins: Vec<Message>,
    members: Vec<gateway::MemberRow>,
    roles: HashMap<String, api::Role>,
    member_roles: HashMap<String, Vec<String>>,
    presence: HashMap<String, String>,
    session_id: Option<String>,
    // voice
    voice: Option<voice::VoiceHandle>,
    voice_settings: voice::VoiceSettings,
    voice_target: Option<(Option<String>, String)>,
    voice_session: Option<String>,
    voice_server: Option<(String, String)>,
    voice_connected: bool,
    voice_status: String,
    voice_privacy: Option<String>,
    /// user id -> (guild id, channel id, self-muted)
    voice_states: HashMap<String, (Option<String>, String, bool)>,
    speaking: HashSet<String>,
    audio_devices: Option<(Vec<String>, Vec<String>)>,
    inbox_open: bool,
    inbox: Vec<Message>,
    inbox_loading: bool,
    status_menu: bool,
    my_status: String,
    settings_tab: u8,
    /// Divider "Nouveaux messages" goes before this message id.
    new_since: Option<String>,
    jump_to: Option<String>,
    flash: Option<(String, Instant)>,
    friends_all: bool,
    /// Channel to open once the guild's channels are loaded.
    pending_channel: Option<String>,
    all_roles: HashMap<String, HashMap<String, api::Role>>,
    users: HashMap<String, User>,
    profile_data: Option<api::Profile>,
    profile_loading: bool,
    profile_err: String,
    /// Channels whose history we may not read / post in (HTTP 403).
    forbidden: HashSet<String>,
    no_send: HashSet<String>,
    loading_messages: bool,
    width: f32,
    height: f32,
    nav_open: bool,
    status_at: Option<Instant>,
    last_status_seen: String,
    threads: Vec<Channel>,
    forum_archived: HashMap<String, Vec<Channel>>,
    picker_tab: PickerTab,
    gif_query: String,
    gif_typing: bool,
    gifs: Vec<api::Gif>,
    gif_note: String,
    commands: HashMap<String, Vec<api::SlashCommand>>,
    command_apps: HashMap<String, String>,
    slash_sel: usize,
    window_active: bool,
    guild_emojis: HashMap<String, Vec<GuildEmoji>>,
    profile: Option<User>,
    settings: bool,
    title_badge: bool,
    theme: ui::color::Theme,
    /// "compact" | "default" | "spacious".
    density: String,
    sidebar_width: f32,
    resizing: bool,
    notifications: bool,
    last_title: String,
    gateway_cmd: Option<flume::Sender<gateway::Command>>,
    last_member_req: Option<Instant>,
    loading_older: bool,
    /// True once the oldest message of the channel has been loaded.
    history_done: bool,
    replying: Option<Message>,
    /// Id of the message being edited; the composer holds its new text.
    editing: Option<String>,
    confirm_delete: Option<String>,
    known_users: HashMap<String, String>,
    typing: HashMap<(String, String), Instant>,
    last_typing_sent: Option<Instant>,
    images: HashMap<String, Arc<Image>>,
    image_pending: HashSet<String>,
    image_failed: HashSet<String>,
    // login screen
    field: Field,
    login_mode: LoginMode,
    form: [String; 4],
    mfa_ticket: String,
    login_busy: bool,
    autologin: bool,
    qr: QrState,
    qr_cells: Vec<Vec<bool>>,
    qr_stop: Arc<AtomicBool>,
    // extras
    unread: HashMap<String, Unread>,
    friends: Vec<User>,
    picker: Option<Picker>,
    switcher: Option<String>,
    switcher_sel: usize,
}

/// The text a keystroke types, if any. Space arrives as key "space" on some
/// platforms with no `key_char`; AltGr (ctrl+alt) still types.
fn typed(ks: &gpui::Keystroke) -> Option<String> {
    if (ks.modifiers.control && !ks.modifiers.alt) || ks.modifiers.platform {
        return None;
    }
    match (&ks.key_char, ks.key.as_str()) {
        (Some(c), _) if !c.is_empty() && !c.chars().all(char::is_control) => Some(c.clone()),
        (_, "space") => Some(" ".into()),
        _ => None,
    }
}

fn snowflake(id: &str) -> u64 {
    id.parse().unwrap_or(0)
}

fn detect_format(b: &[u8]) -> Option<ImageFormat> {
    if b.starts_with(&[0x89, b'P', b'N', b'G']) {
        Some(ImageFormat::Png)
    } else if b.starts_with(&[0xff, 0xd8]) {
        Some(ImageFormat::Jpeg)
    } else if b.starts_with(b"GIF8") {
        Some(ImageFormat::Gif)
    } else if b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WEBP" {
        Some(ImageFormat::Webp)
    } else {
        None
    }
}

impl DiscordApp {
    fn new(cx: &mut Context<Self>) -> Self {
        let prefs = prefs::load();
        let theme = ui::color::Theme::from_key(&prefs.theme);
        ui::color::set_theme(theme);
        let mut this = Self {
            focus: cx.focus_handle(),
            scroll: ScrollHandle::new(),
            auth: String::new(),
            me: None,
            logged_in: false,
            connected: false,
            gateway_stop: Arc::new(AtomicBool::new(false)),
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
            show_side: prefs.show_side,
            side: Side::Members,
            search: None,
            search_results: vec![],
            search_note: String::new(),
            pins: vec![],
            members: vec![],
            roles: HashMap::new(),
            member_roles: HashMap::new(),
            presence: HashMap::new(),
            session_id: None,
            voice: None,
            voice_settings: {
                let v = voice::VoiceSettings {
                    input_device: prefs.input_device.clone(),
                    output_device: prefs.output_device.clone(),
                    ..Default::default()
                };
                v.noise_suppression
                    .store(prefs.noise_suppression, Ordering::Relaxed);
                v
            },
            voice_target: None,
            voice_session: None,
            voice_server: None,
            voice_connected: false,
            voice_status: String::new(),
            voice_privacy: None,
            voice_states: HashMap::new(),
            speaking: HashSet::new(),
            audio_devices: None,
            inbox_open: false,
            inbox: vec![],
            inbox_loading: false,
            status_menu: false,
            my_status: "online".into(),
            settings_tab: 0,
            new_since: None,
            jump_to: None,
            flash: None,
            friends_all: false,
            pending_channel: None,
            all_roles: HashMap::new(),
            users: HashMap::new(),
            profile_data: None,
            profile_loading: false,
            profile_err: String::new(),
            forbidden: HashSet::new(),
            no_send: HashSet::new(),
            loading_messages: false,
            width: 1280.0,
            height: 780.0,
            nav_open: false,
            status_at: None,
            last_status_seen: String::new(),
            threads: vec![],
            forum_archived: HashMap::new(),
            picker_tab: PickerTab::Emoji,
            gif_query: String::new(),
            gif_typing: false,
            gifs: vec![],
            gif_note: String::new(),
            commands: HashMap::new(),
            command_apps: HashMap::new(),
            slash_sel: 0,
            window_active: true,
            guild_emojis: HashMap::new(),
            profile: None,
            settings: false,
            title_badge: prefs.title_badge,
            theme,
            density: prefs.density.clone(),
            sidebar_width: prefs.sidebar_width.clamp(180., 420.),
            resizing: false,
            notifications: prefs.notifications,
            last_title: String::new(),
            gateway_cmd: None,
            last_member_req: None,
            loading_older: false,
            history_done: false,
            replying: None,
            editing: None,
            confirm_delete: None,
            known_users: HashMap::new(),
            typing: HashMap::new(),
            last_typing_sent: None,
            images: HashMap::new(),
            image_pending: HashSet::new(),
            image_failed: HashSet::new(),
            field: Field::Email,
            login_mode: LoginMode::Credentials,
            form: Default::default(),
            mfa_ticket: String::new(),
            login_busy: false,
            autologin: false,
            qr: QrState::Loading,
            qr_cells: vec![],
            qr_stop: Arc::new(AtomicBool::new(false)),
            unread: HashMap::new(),
            friends: vec![],
            picker: None,
            switcher: None,
            switcher_sel: 0,
        };
        if std::env::var_os("DISCORD_DEMO").is_some() {
            this.load_demo();
            return this;
        }
        let env = std::env::var("DISCORD_TOKEN")
            .ok()
            .filter(|t| !t.trim().is_empty());
        match env.or_else(store::load) {
            Some(token) => {
                this.autologin = true;
                this.login(token.trim().to_string(), cx);
            }
            None => this.start_qr(cx),
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

    fn set_input(&mut self, s: String) {
        self.cursor = s.chars().count();
        self.input = s;
    }

    /// Inserts an empty `m…m` pair with the caret between the markers.
    fn wrap_markers(&mut self, m: &str) {
        self.insert(&format!("{m}{m}"));
        self.cursor -= m.chars().count();
    }

    fn clear_input(&mut self) {
        self.set_input(String::new());
    }

    fn cancel_compose(&mut self) {
        if self.editing.take().is_some() {
            self.clear_input();
        }
        self.replying = None;
    }

    fn on_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let cmd = ks.modifiers.control || ks.modifiers.platform;
        if self.logged_in && cmd && ks.key == "k" {
            self.switcher = match self.switcher {
                Some(_) => None,
                None => Some(String::new()),
            };
            self.switcher_sel = 0;
            self.picker = None;
            return cx.notify();
        }
        if ks.key == "escape" && (self.profile.is_some() || self.settings) {
            self.profile = None;
            self.settings = false;
            return cx.notify();
        }
        if self.logged_in && cmd && ks.key == "f" {
            self.search = match self.search {
                Some(_) => None,
                None => Some(String::new()),
            };
            self.side = Side::Search;
            self.show_side = true;
            return cx.notify();
        }
        if self.switcher.is_some() {
            return self.on_switcher_key(ev, cx);
        }
        if self.picker.is_some() && self.picker_tab == PickerTab::Gif && self.gif_typing {
            return self.on_gif_key(ev, cx);
        }
        if self.search.is_some() {
            return self.on_search_key(ev, cx);
        }
        let len = self.input.chars().count();
        let before = self.input.len();
        if self.logged_in {
            let sugg = self.slash_suggestions();
            if !sugg.is_empty() {
                match ks.key.as_str() {
                    "up" => {
                        self.slash_sel = self.slash_sel.saturating_sub(1);
                        return cx.notify();
                    }
                    "down" => {
                        self.slash_sel = (self.slash_sel + 1).min(sugg.len() - 1);
                        return cx.notify();
                    }
                    "tab" | "enter" => {
                        let pick = &sugg[self.slash_sel.min(sugg.len() - 1)];
                        self.set_input(format!("/{} ", pick.name));
                        self.slash_sel = 0;
                        return cx.notify();
                    }
                    _ => {}
                }
            }
        }
        match ks.key.as_str() {
            "tab" if !self.logged_in => self.cycle_field(),
            "enter" if ks.modifiers.shift => self.insert("\n"),
            "enter" => return self.submit(cx),
            "escape" if self.picker.is_some() => self.picker = None,
            "escape" => self.cancel_compose(),
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
            "up" if self.input.is_empty() && self.editing.is_none() => self.edit_last_own(),
            "v" if cmd => {
                if let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()) {
                    self.insert(&text.replace('\r', ""));
                }
            }
            "a" if cmd => self.cursor = len,
            "b" if cmd => self.wrap_markers("**"),
            "i" if cmd => self.wrap_markers("*"),
            "u" if cmd => self.wrap_markers("__"),
            "e" if cmd => self.wrap_markers("`"),
            "x" if cmd && ks.modifiers.shift => self.wrap_markers("~~"),
            "c" if cmd => cx.write_to_clipboard(ClipboardItem::new_string(self.input.clone())),
            _ if cmd => return,
            _ => match typed(ks) {
                Some(ch) => self.insert(&ch),
                None => return,
            },
        }
        if self.input.len() > before {
            self.send_typing(cx);
        }
        self.slash_sel = 0;
        if self.input.starts_with('/') {
            self.ensure_commands(cx);
        }
        cx.notify();
    }

    // ---- login form ------------------------------------------------------

    pub fn save_prefs(&self) {
        prefs::save(&prefs::Prefs {
            theme: self.theme.key().into(),
            density: self.density.clone(),
            sidebar_width: self.sidebar_width,
            noise_suppression: self
                .voice_settings
                .noise_suppression
                .load(Ordering::Relaxed),
            input_device: self.voice_settings.input_device.clone(),
            output_device: self.voice_settings.output_device.clone(),
            light: false,
            notifications: self.notifications,
            title_badge: self.title_badge,
            show_side: self.show_side,
        });
    }

    pub fn set_theme(&mut self, theme: ui::color::Theme, cx: &mut Context<Self>) {
        self.theme = theme;
        ui::color::set_theme(theme);
        self.save_prefs();
        cx.notify();
    }

    pub fn set_density(&mut self, density: &str, cx: &mut Context<Self>) {
        self.density = density.to_string();
        self.save_prefs();
        cx.notify();
    }

    /// Offline fixtures for `DISCORD_DEMO=1`.
    fn load_demo(&mut self) {
        let d = demo::data();
        self.me = Some(d.me);
        self.guilds = d.guilds;
        self.dms = d.dms;
        self.friends = d.friends;
        self.logged_in = true;
        self.connected = true;
        self.guild = self.guilds.first().map(|g| g.id.clone());
        self.channels = d.channels;
        self.channel = self.channels.iter().find(|c| c.kind == 0).cloned();
        self.learn_users(&d.messages);
        self.messages = d.messages;
        self.history_done = true;
        for (id, st) in [
            ("100000000000000002", "online"),
            ("100000000000000003", "idle"),
            ("100000000000000004", "dnd"),
        ] {
            self.presence.insert(id.into(), st.into());
        }
        self.unread.insert(
            "300000000000000004".into(),
            Unread {
                guild: self.guild.clone(),
                mentions: 2,
                first_id: None,
            },
        );
        self.unread.insert(
            "300000000000000099".into(),
            Unread {
                guild: Some("200000000000000003".into()),
                mentions: 0,
                first_id: None,
            },
        );
        // DISCORD_DEMO_VIEW=settings:1,theme:light,profile,friends,inbox,status,picker,switcher
        let view = std::env::var("DISCORD_DEMO_VIEW").unwrap_or_default();
        for part in view.split(',') {
            let (k, v) = part.split_once(':').unwrap_or((part, ""));
            match k {
                "settings" => {
                    self.settings = true;
                    self.settings_tab = v.parse().unwrap_or(0);
                }
                "theme" => {
                    self.theme = ui::color::Theme::from_key(v);
                    ui::color::set_theme(self.theme);
                }
                "profile" => self.profile = self.users.get("100000000000000002").cloned(),
                "friends" => {
                    self.guild = None;
                    self.channel = None;
                }
                "dm" => {
                    self.guild = None;
                    self.channel = self.dms.first().cloned();
                }
                "inbox" => {
                    self.inbox_open = true;
                    self.inbox = self.messages.iter().rev().take(2).cloned().collect();
                }
                "status" => self.status_menu = true,
                "picker" => self.picker = Some(Picker::Composer),
                "switcher" => self.switcher = Some("g".into()),
                "reply" => self.replying = self.messages.get(1).cloned(),
                "density" => self.density = v.to_string(),
                "voice" => {
                    let g = self.guild.clone();
                    for (u, m) in [
                        ("100000000000000001", false),
                        ("100000000000000002", false),
                        ("100000000000000003", true),
                    ] {
                        self.voice_states
                            .insert(u.into(), (g.clone(), "300000000000000008".into(), m));
                    }
                    self.speaking.insert("100000000000000002".into());
                    self.voice_target = Some((g, "300000000000000008".into()));
                    self.voice_connected = true;
                    self.voice_status = "Voix connectée".into();
                    self.voice_privacy = Some("12345 67890 13579".into());
                }
                _ => {}
            }
        }
    }

    fn sync_field(&mut self) {
        self.form[self.field as usize] = self.input.clone();
    }

    pub fn set_field(&mut self, f: Field) {
        self.sync_field();
        self.field = f;
        self.set_input(self.form[f as usize].clone());
    }

    fn cycle_field(&mut self) {
        let next = match (self.login_mode, self.field) {
            (LoginMode::Credentials, Field::Email) => Field::Password,
            _ => Field::Email,
        };
        if self.login_mode == LoginMode::Credentials {
            self.set_field(next);
        }
    }

    pub fn set_mode(&mut self, mode: LoginMode, cx: &mut Context<Self>) {
        self.sync_field();
        self.login_mode = mode;
        let f = match mode {
            LoginMode::Credentials => Field::Email,
            LoginMode::Token => Field::Token,
            LoginMode::Mfa => Field::Code,
        };
        self.field = f;
        self.set_input(self.form[f as usize].clone());
        self.status.clear();
        cx.notify();
    }

    pub fn submit_login(&mut self, cx: &mut Context<Self>) {
        if self.login_busy {
            return;
        }
        self.sync_field();
        match self.login_mode {
            LoginMode::Credentials => {
                if self.field == Field::Email {
                    return self.set_field(Field::Password);
                }
                let (email, pw) = (
                    self.form[Field::Email as usize].trim().to_string(),
                    self.form[Field::Password as usize].clone(),
                );
                if email.is_empty() || pw.is_empty() {
                    self.status = "Renseignez votre e-mail et votre mot de passe.".into();
                    return;
                }
                self.login_busy = true;
                self.status = "Connexion…".into();
                cx.spawn(async move |this, cx| {
                    let res = cx
                        .background_spawn(async move { api::password_login(&email, &pw) })
                        .await;
                    this.update(cx, |this, cx| {
                        this.login_busy = false;
                        match res {
                            Ok(api::PasswordLogin::Token(t)) => this.login(t, cx),
                            Ok(api::PasswordLogin::Mfa(ticket)) => {
                                this.mfa_ticket = ticket;
                                this.set_mode(LoginMode::Mfa, cx);
                            }
                            Ok(api::PasswordLogin::Captcha) => {
                                this.status =
                                    "Discord demande un captcha : utilisez le code QR ou un token."
                                        .into();
                            }
                            Err(e) => this.status = e,
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
            LoginMode::Token => {
                let t = self.form[Field::Token as usize].trim().to_string();
                if !t.is_empty() {
                    self.login(t, cx);
                }
            }
            LoginMode::Mfa => {
                let code = self.form[Field::Code as usize].trim().to_string();
                let ticket = self.mfa_ticket.clone();
                if code.is_empty() {
                    return;
                }
                self.login_busy = true;
                self.status = "Vérification…".into();
                cx.spawn(async move |this, cx| {
                    let res = cx
                        .background_spawn(async move { api::mfa_totp(&code, &ticket) })
                        .await;
                    this.update(cx, |this, cx| {
                        this.login_busy = false;
                        match res {
                            Ok(t) => this.login(t, cx),
                            Err(e) => this.status = e,
                        }
                        cx.notify();
                    })
                    .ok();
                })
                .detach();
            }
        }
        cx.notify();
    }

    pub fn start_qr(&mut self, cx: &mut Context<Self>) {
        self.qr_stop.store(true, Ordering::Relaxed);
        self.qr_stop = Arc::new(AtomicBool::new(false));
        self.qr = QrState::Loading;
        let (tx, rx) = flume::unbounded();
        qr::spawn(tx, self.qr_stop.clone());
        cx.spawn(async move |this, cx| {
            while let Ok(ev) = rx.recv_async().await {
                let done = matches!(ev, qr::QrEvent::Token(_) | qr::QrEvent::Failed(_));
                let alive = this.update(cx, |this, cx| {
                    match ev {
                        qr::QrEvent::Code(url) => {
                            if let Ok(code) = qrcode::QrCode::new(url.as_bytes()) {
                                let w = code.width();
                                let colors = code.to_colors();
                                this.qr_cells = colors
                                    .chunks(w)
                                    .map(|row| {
                                        row.iter().map(|c| *c == qrcode::Color::Dark).collect()
                                    })
                                    .collect();
                                this.qr = QrState::Ready;
                            }
                        }
                        qr::QrEvent::Scanned(name) => this.qr = QrState::Scanned(name),
                        qr::QrEvent::Token(t) => this.login(t, cx),
                        qr::QrEvent::Failed(e) => this.qr = QrState::Failed(e),
                    }
                    cx.notify();
                });
                if alive.is_err() || done {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    // ---- quick switcher / pickers / files -----------------------------------

    pub fn switcher_items(&self) -> Vec<(String, Target)> {
        let q = self.switcher.clone().unwrap_or_default().to_lowercase();
        let mut items: Vec<(String, Target)> =
            vec![("Amis · Messages privés".into(), Target::Home)];
        items.extend(
            self.guilds
                .iter()
                .map(|g| (g.name.clone(), Target::Guild(g.id.clone()))),
        );
        items.extend(
            self.dms
                .iter()
                .map(|c| (format!("@{}", c.title()), Target::Channel(c.clone()))),
        );
        items.extend(
            self.channels
                .iter()
                .filter(|c| c.kind != 4)
                .map(|c| (format!("#{}", c.title()), Target::Channel(c.clone()))),
        );
        items.retain(|(label, _)| label.to_lowercase().contains(&q));
        items.truncate(12);
        items
    }

    fn on_switcher_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        let items = self.switcher_items();
        match ks.key.as_str() {
            "escape" => self.switcher = None,
            "up" => self.switcher_sel = self.switcher_sel.saturating_sub(1),
            "down" => {
                self.switcher_sel = (self.switcher_sel + 1).min(items.len().saturating_sub(1))
            }
            "backspace" => {
                if let Some(q) = &mut self.switcher {
                    q.pop();
                }
                self.switcher_sel = 0;
            }
            "enter" => {
                if let Some((_, target)) = items.get(self.switcher_sel).cloned() {
                    self.go(target, cx);
                }
            }
            _ => {
                if ks.modifiers.control || ks.modifiers.platform {
                    return;
                }
                if let (Some(ch), Some(q)) = (typed(ks), &mut self.switcher) {
                    q.push_str(&ch);
                    self.switcher_sel = 0;
                }
            }
        }
        cx.notify();
    }

    fn on_search_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        match ks.key.as_str() {
            "escape" => self.search = None,
            "backspace" => {
                if let Some(q) = &mut self.search {
                    q.pop();
                }
            }
            "enter" => self.run_search(cx),
            _ => {
                if ks.modifiers.control || ks.modifiers.platform {
                    return;
                }
                if let (Some(ch), Some(q)) = (typed(ks), &mut self.search) {
                    q.push_str(&ch);
                }
            }
        }
        cx.notify();
    }

    fn run_search(&mut self, cx: &mut Context<Self>) {
        let (Some(query), Some(channel)) = (self.search.clone(), self.channel.clone()) else {
            return;
        };
        if query.trim().is_empty() {
            return;
        }
        let (auth, guild) = (self.auth.clone(), self.guild.clone());
        self.search_note = "Recherche…".into();
        self.search_results.clear();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    api::search(&auth, guild.as_deref(), &channel.id, query.trim())
                })
                .await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(r) => {
                        this.search_note = if r.is_empty() {
                            "Aucun résultat.".into()
                        } else {
                            String::new()
                        };
                        this.search_results = r;
                    }
                    Err(e) => this.search_note = e,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn open_pins(&mut self, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        self.side = Side::Pins;
        self.show_side = true;
        self.pins.clear();
        let auth = self.auth.clone();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::pins(&auth, &channel.id) })
                .await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(p) => this.pins = p,
                    Err(e) => this.status = format!("Épingles indisponibles : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn show_members_panel(&mut self, cx: &mut Context<Self>) {
        if self.show_side && self.side == Side::Members {
            self.show_side = false;
        } else {
            self.side = Side::Members;
            self.show_side = true;
            self.request_members(cx);
        }
        cx.notify();
    }

    /// Asks the gateway for the guild's member list (with presence). User accounts only.
    fn request_members(&mut self, _cx: &mut Context<Self>) {
        let (Some(guild), Some(channel), Some(tx)) = (
            self.guild.clone(),
            self.channel.clone(),
            self.gateway_cmd.clone(),
        ) else {
            return;
        };
        if self
            .last_member_req
            .is_some_and(|t| t.elapsed() < Duration::from_secs(4))
        {
            return;
        }
        self.last_member_req = Some(Instant::now());
        let _ = tx.send(gateway::Command::RequestMembers {
            guild_id: guild.clone(),
            channel_id: channel.id,
        });
    }

    fn fetch_roles(&mut self, guild: String, cx: &mut Context<Self>) {
        let auth = self.auth.clone();
        cx.spawn(async move |this, cx| {
            let g = guild.clone();
            if let Ok(r) = cx
                .background_spawn(async move { api::roles(&auth, &g) })
                .await
            {
                this.update(cx, |this, cx| {
                    this.all_roles.insert(guild.clone(), r.clone());
                    if this.guild.as_deref() == Some(&guild) {
                        this.roles = r;
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    /// Colour of the member's highest coloured role, if any.
    pub fn name_color(&self, user_id: &str) -> Option<u32> {
        self.member_roles
            .get(user_id)?
            .iter()
            .filter_map(|id| self.roles.get(id))
            .filter(|r| r.color != 0)
            .max_by_key(|r| r.position)
            .map(|r| r.color)
    }

    pub fn refresh_threads(&mut self, cx: &mut Context<Self>) {
        let Some(guild) = self.guild.clone() else {
            return;
        };
        let auth = self.auth.clone();
        cx.spawn(async move |this, cx| {
            let g = guild.clone();
            if let Ok(t) = cx
                .background_spawn(async move { api::active_threads(&auth, &g) })
                .await
            {
                this.update(cx, |this, cx| {
                    if this.guild.as_deref() == Some(&guild) {
                        this.threads = t;
                        cx.notify();
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    fn load_archived(&mut self, forum: String, cx: &mut Context<Self>) {
        let auth = self.auth.clone();
        cx.spawn(async move |this, cx| {
            let f = forum.clone();
            if let Ok(t) = cx
                .background_spawn(async move { api::archived_threads(&auth, &f) })
                .await
            {
                this.update(cx, |this, cx| {
                    this.forum_archived.insert(forum, t);
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
    }

    /// Creates a thread from a message and opens it.
    pub fn start_thread(&mut self, m: &Message, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        let name: String = m
            .content
            .lines()
            .next()
            .unwrap_or("")
            .chars()
            .take(60)
            .collect();
        let name = if name.trim().is_empty() {
            "Nouveau fil".to_string()
        } else {
            name
        };
        let (auth, mid) = (self.auth.clone(), m.id.clone());
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::start_thread(&auth, &channel.id, &mid, &name) })
                .await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(t) => {
                        this.threads.push(t.clone());
                        this.select_channel(t, cx);
                    }
                    Err(e) => this.status = format!("Création du fil impossible : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Fetches the channel's application commands the first time `/` is typed.
    fn ensure_commands(&mut self, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        if self.commands.contains_key(&channel.id) || self.auth.starts_with("Bot ") {
            return;
        }
        self.commands.insert(channel.id.clone(), vec![]);
        let auth = self.auth.clone();
        cx.spawn(async move |this, cx| {
            let cid = channel.id.clone();
            let res = cx
                .background_spawn(async move { api::command_index(&auth, &cid) })
                .await;
            this.update(cx, |this, cx| {
                if let Ok((cmds, apps)) = res {
                    this.commands.insert(channel.id, cmds);
                    this.command_apps.extend(apps);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Autocomplete rows while the user is typing `/name` (before the first space).
    pub fn slash_suggestions(&self) -> Vec<SlashEntry> {
        let Some(rest) = self.input.strip_prefix('/') else {
            return vec![];
        };
        if rest.contains(char::is_whitespace) || self.channel.is_none() {
            return vec![];
        }
        let q = rest.to_lowercase();
        let mut out: Vec<SlashEntry> = BUILTIN_COMMANDS
            .iter()
            .filter(|(n, _)| n.starts_with(&q))
            .map(|(n, d)| SlashEntry {
                name: n.to_string(),
                desc: d.to_string(),
                app: "Discord".into(),
            })
            .collect();
        if let Some(list) = self.channel.as_ref().and_then(|c| self.commands.get(&c.id)) {
            out.extend(
                list.iter()
                    .filter(|c| c.name.to_lowercase().contains(&q))
                    .map(|c| SlashEntry {
                        name: c.name.clone(),
                        desc: c.description.clone(),
                        app: self
                            .command_apps
                            .get(&c.application_id)
                            .cloned()
                            .unwrap_or_default(),
                    }),
            );
        }
        out.truncate(8);
        out
    }

    /// Handles `/command args`. Returns the text to send as a plain message, or
    /// `None` when the command was dispatched as an interaction (or failed).
    fn handle_slash(&mut self, text: &str, cx: &mut Context<Self>) -> Option<String> {
        let body = text.strip_prefix('/')?;
        let (name, args) = body.split_once(char::is_whitespace).unwrap_or((body, ""));
        let args = args.trim();
        match name {
            "shrug" => return Some(format!("{args} ¯\\_(ツ)_/¯").trim().to_string()),
            "tableflip" => return Some(format!("{args} (╯°□°)╯︵ ┻━┻").trim().to_string()),
            "unflip" => return Some(format!("{args} ┬─┬ ノ( ゜-゜ノ)").trim().to_string()),
            "me" => return Some(format!("_{args}_")),
            "spoiler" => return Some(format!("||{args}||")),
            _ => {}
        }
        let channel = self.channel.clone()?;
        let cmd = self
            .commands
            .get(&channel.id)
            .and_then(|l| l.iter().find(|c| c.name == name))
            .cloned();
        let Some(cmd) = cmd else {
            return Some(text.to_string());
        };
        let Some(session) = self.session_id.clone() else {
            self.status = "Session Gateway indisponible, réessayez dans un instant.".into();
            return None;
        };
        if cmd.options.iter().any(|o| o.kind < 3 || o.kind > 10) {
            self.status = format!("/{name} utilise des options non prises en charge.");
            return None;
        }
        // Positional arguments: the last option takes the remainder.
        let mut options = Vec::new();
        let mut rest = args;
        for (i, o) in cmd.options.iter().enumerate() {
            let last = i + 1 == cmd.options.len();
            let (tok, tail) = if last || o.kind != 3 {
                match rest.split_once(char::is_whitespace) {
                    Some((a, b)) if !last => (a, b.trim_start()),
                    _ => (rest, ""),
                }
            } else {
                rest.split_once(char::is_whitespace).unwrap_or((rest, ""))
            };
            rest = tail;
            if tok.is_empty() {
                if o.required {
                    self.status = format!("Option requise manquante : {}", o.name);
                    return None;
                }
                continue;
            }
            let value = match o.kind {
                3 => serde_json::json!(tok),
                4 => match tok.parse::<i64>() {
                    Ok(n) => serde_json::json!(n),
                    Err(_) => {
                        self.status = format!("{} doit être un entier.", o.name);
                        return None;
                    }
                },
                10 => match tok.parse::<f64>() {
                    Ok(n) => serde_json::json!(n),
                    Err(_) => {
                        self.status = format!("{} doit être un nombre.", o.name);
                        return None;
                    }
                },
                5 => serde_json::json!(matches!(tok, "true" | "oui" | "1")),
                _ => {
                    self.status = format!("L'option {} n'est pas prise en charge.", o.name);
                    return None;
                }
            };
            options.push(serde_json::json!({ "type": o.kind, "name": o.name, "value": value }));
        }
        let (auth, guild) = (self.auth.clone(), self.guild.clone());
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    api::run_command(
                        &auth,
                        &session,
                        guild.as_deref(),
                        &channel.id,
                        &cmd,
                        options,
                    )
                })
                .await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    this.status = format!("Commande impossible : {e}");
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        None
    }

    fn on_gif_key(&mut self, ev: &KeyDownEvent, cx: &mut Context<Self>) {
        let ks = &ev.keystroke;
        match ks.key.as_str() {
            "escape" => self.picker = None,
            "backspace" => {
                self.gif_query.pop();
            }
            "enter" => self.load_gifs(cx),
            _ => {
                if ks.modifiers.control || ks.modifiers.platform {
                    return;
                }
                if let Some(ch) = typed(ks) {
                    self.gif_query.push_str(&ch);
                }
            }
        }
        cx.notify();
    }

    pub fn open_gif_tab(&mut self, cx: &mut Context<Self>) {
        self.picker_tab = PickerTab::Gif;
        self.gif_typing = true;
        if self.gifs.is_empty() {
            self.load_gifs(cx);
        }
        cx.notify();
    }

    pub fn load_gifs(&mut self, cx: &mut Context<Self>) {
        let (auth, q) = (self.auth.clone(), self.gif_query.clone());
        self.gif_note = "Chargement…".into();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::gifs(&auth, &q) })
                .await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(g) => {
                        this.gif_note = if g.is_empty() {
                            "Aucun GIF trouvé.".into()
                        } else {
                            String::new()
                        };
                        this.gifs = g;
                    }
                    Err(e) => this.gif_note = format!("GIF indisponibles : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn send_gif(&mut self, url: String, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        self.picker = None;
        let reply = self.replying.take().map(|m| m.id);
        let auth = self.auth.clone();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(
                    async move { api::send(&auth, &channel.id, &url, reply.as_deref()) },
                )
                .await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    if e.contains("(403)") {
                        if let Some(c) = &this.channel {
                            this.no_send.insert(c.id.clone());
                        }
                    }
                    this.status = format!("Échec de l'envoi : {e}");
                }
                this.last_msg_id = None;
                this.refresh_messages(cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Opens the profile popup and loads the full profile in the background.
    pub fn open_profile(&mut self, user: User, cx: &mut Context<Self>) {
        self.users.insert(user.id.clone(), user.clone());
        self.profile = Some(user.clone());
        self.profile_data = None;
        self.profile_err.clear();
        if self.auth.starts_with("Bot ") {
            return cx.notify();
        }
        self.profile_loading = true;
        let (auth, guild, id) = (self.auth.clone(), self.guild.clone(), user.id);
        cx.spawn(async move |this, cx| {
            let uid = id.clone();
            let res = cx
                .background_spawn(async move { api::profile(&auth, &uid, guild.as_deref()) })
                .await;
            this.update(cx, |this, cx| {
                if this.profile.as_ref().map(|u| &u.id) != Some(&id) {
                    return;
                }
                this.profile_loading = false;
                match res {
                    Ok(p) => this.profile_data = Some(p),
                    Err(e) => this.profile_err = e,
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn toggle_inbox(&mut self, cx: &mut Context<Self>) {
        self.inbox_open = !self.inbox_open;
        self.status_menu = false;
        if self.inbox_open && !self.auth.starts_with("Bot ") {
            self.inbox_loading = true;
            let auth = self.auth.clone();
            cx.spawn(async move |this, cx| {
                let res = cx
                    .background_spawn(async move { api::mentions(&auth) })
                    .await;
                this.update(cx, |this, cx| {
                    this.inbox_loading = false;
                    match res {
                        Ok(m) => {
                            this.learn_users(&m);
                            this.inbox = m;
                        }
                        Err(e) => this.status = format!("Mentions indisponibles : {e}"),
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    /// Opens the channel of a message (possibly in another guild) and jumps to it.
    pub fn open_message(&mut self, m: &Message, cx: &mut Context<Self>) {
        self.inbox_open = false;
        self.jump_to = Some(m.id.clone());
        match &m.guild_id {
            Some(g) if self.guild.as_deref() != Some(g.as_str()) => {
                self.pending_channel = Some(m.channel_id.clone());
                self.select_guild(g.clone(), cx);
            }
            Some(_) => self.open_channel_id(&m.channel_id.clone(), cx),
            None => {
                self.guild = None;
                self.channels.clear();
                self.open_channel_id(&m.channel_id.clone(), cx);
            }
        }
        cx.notify();
    }

    pub fn set_status(&mut self, status: &str, cx: &mut Context<Self>) {
        self.my_status = status.to_string();
        self.status_menu = false;
        if let Some(tx) = &self.gateway_cmd {
            let _ = tx.send(gateway::Command::SetStatus(status.to_string()));
        }
        cx.notify();
    }

    /// Scrolls to a loaded message (reply previews) and flashes it.
    pub fn jump_to_message(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.messages.iter().any(|m| m.id == id) {
            self.jump_to = Some(id.to_string());
        } else {
            self.status =
                "Ce message n'est pas chargé : utilisez « Charger les messages précédents »."
                    .into();
        }
        cx.notify();
    }

    // ---- voice -----------------------------------------------------------------

    /// Joins a voice channel (guild) or call (DM: `guild = None`).
    pub fn join_voice(&mut self, guild: Option<String>, channel: String, cx: &mut Context<Self>) {
        if self
            .voice_target
            .as_ref()
            .is_some_and(|(_, c)| *c == channel)
        {
            return;
        }
        if self.auth.starts_with("Bot ") || self.gateway_cmd.is_none() {
            self.status = "Le vocal nécessite une connexion au Gateway.".into();
            return cx.notify();
        }
        self.voice = None;
        self.voice_server = None;
        self.voice_session = None;
        self.voice_connected = false;
        self.voice_privacy = None;
        self.speaking.clear();
        self.voice_target = Some((guild.clone(), channel.clone()));
        self.voice_status = "Connexion…".into();
        if let Some(tx) = &self.gateway_cmd {
            let _ = tx.send(gateway::Command::VoiceState {
                guild_id: guild,
                channel_id: Some(channel),
                mute: self.voice_settings.muted.load(Ordering::Relaxed),
                deaf: self.voice_settings.deafened.load(Ordering::Relaxed),
            });
        }
        cx.notify();
    }

    pub fn leave_voice(&mut self, cx: &mut Context<Self>) {
        let Some((guild, _)) = self.voice_target.take() else {
            return;
        };
        self.voice = None;
        self.voice_connected = false;
        self.voice_privacy = None;
        self.speaking.clear();
        self.voice_status.clear();
        if let Some(tx) = &self.gateway_cmd {
            let _ = tx.send(gateway::Command::VoiceState {
                guild_id: guild,
                channel_id: None,
                mute: false,
                deaf: false,
            });
        }
        cx.notify();
    }

    fn send_voice_flags(&self) {
        if let (Some((guild, channel)), Some(tx)) = (&self.voice_target, &self.gateway_cmd) {
            let _ = tx.send(gateway::Command::VoiceState {
                guild_id: guild.clone(),
                channel_id: Some(channel.clone()),
                mute: self.voice_settings.muted.load(Ordering::Relaxed),
                deaf: self.voice_settings.deafened.load(Ordering::Relaxed),
            });
        }
    }

    pub fn toggle_mute(&mut self, cx: &mut Context<Self>) {
        let m = &self.voice_settings.muted;
        m.store(!m.load(Ordering::Relaxed), Ordering::Relaxed);
        self.send_voice_flags();
        cx.notify();
    }

    pub fn toggle_deafen(&mut self, cx: &mut Context<Self>) {
        let d = &self.voice_settings.deafened;
        let now = !d.load(Ordering::Relaxed);
        d.store(now, Ordering::Relaxed);
        // Deafening also mutes, like Discord.
        if now {
            self.voice_settings.muted.store(true, Ordering::Relaxed);
        }
        self.send_voice_flags();
        cx.notify();
    }

    pub fn toggle_noise_suppression(&mut self, cx: &mut Context<Self>) {
        let n = &self.voice_settings.noise_suppression;
        n.store(!n.load(Ordering::Relaxed), Ordering::Relaxed);
        self.save_prefs();
        cx.notify();
    }

    /// Starts the voice connection once both the session and the server are known.
    fn try_start_voice(&mut self, cx: &mut Context<Self>) {
        let (Some((guild, channel)), Some(session), Some((token, endpoint)), Some(me)) = (
            self.voice_target.clone(),
            self.voice_session.clone(),
            self.voice_server.clone(),
            self.me.clone(),
        ) else {
            return;
        };
        if self.voice.is_some() {
            return;
        }
        let info = voice::ConnectInfo {
            endpoint,
            token,
            session_id: session,
            server_id: guild.unwrap_or_else(|| channel.clone()),
            channel_id: channel,
            user_id: me.id,
        };
        let (tx, rx) = flume::unbounded();
        self.voice = Some(voice::connect(info, self.voice_settings.clone(), tx));
        cx.spawn(async move |this, cx| {
            while let Ok(ev) = rx.recv_async().await {
                if this.update(cx, |this, cx| this.on_voice(ev, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_voice(&mut self, ev: voice::VoiceEvent, cx: &mut Context<Self>) {
        use voice::VoiceEvent::*;
        match ev {
            State(s) => self.voice_status = s,
            Connected => {
                self.voice_connected = true;
                self.voice_status = "Voix connectée".into();
            }
            Speaking { user_id, speaking } => {
                let id = if user_id.is_empty() {
                    self.me.as_ref().map(|u| u.id.clone()).unwrap_or_default()
                } else {
                    user_id
                };
                if speaking {
                    self.speaking.insert(id);
                } else {
                    self.speaking.remove(&id);
                }
            }
            Privacy(code) => self.voice_privacy = code,
            Closed(reason) => {
                if self.voice_target.is_some() {
                    self.voice_status = reason;
                }
                self.voice = None;
                self.voice_connected = false;
                self.speaking.clear();
            }
        }
        cx.notify();
    }

    /// Users currently in a voice channel.
    pub fn voice_members(&self, channel: &str) -> Vec<User> {
        let mut ids: Vec<&String> = self
            .voice_states
            .iter()
            .filter(|(_, (_, c, _))| c == channel)
            .map(|(u, _)| u)
            .collect();
        ids.sort();
        ids.into_iter()
            .map(|id| {
                self.users.get(id).cloned().unwrap_or_else(|| User {
                    id: id.clone(),
                    username: self
                        .known_users
                        .get(id)
                        .cloned()
                        .unwrap_or_else(|| "…".into()),
                    ..Default::default()
                })
            })
            .collect()
    }

    pub fn open_channel_id(&mut self, id: &str, cx: &mut Context<Self>) {
        let found = self
            .channels
            .iter()
            .chain(&self.threads)
            .chain(&self.dms)
            .find(|c| c.id == id)
            .cloned();
        if let Some(c) = found {
            self.select_channel(c, cx);
        }
    }

    pub fn open_profile_id(&mut self, id: &str, cx: &mut Context<Self>) {
        let user = self.users.get(id).cloned().unwrap_or_else(|| User {
            id: id.to_string(),
            username: "Utilisateur inconnu".into(),
            ..Default::default()
        });
        self.open_profile(user, cx);
    }

    pub fn pick_custom(&mut self, e: &GuildEmoji, cx: &mut Context<Self>) {
        match self.picker.take() {
            Some(Picker::React(m)) => {
                let emoji = Emoji {
                    id: Some(e.id.clone()),
                    name: Some(e.name.clone()),
                };
                self.toggle_reaction(&m, &emoji, false, cx);
            }
            Some(Picker::Composer) => self.insert(&e.markup()),
            None => {}
        }
        cx.notify();
    }

    pub fn go(&mut self, target: Target, cx: &mut Context<Self>) {
        self.switcher = None;
        match target {
            Target::Home => self.open_home(cx),
            Target::Guild(id) => self.select_guild(id, cx),
            Target::Channel(c) => {
                if c.is_dm() && self.guild.is_some() {
                    self.guild = None;
                    self.channels.clear();
                }
                self.select_channel(c, cx);
            }
        }
        cx.notify();
    }

    pub fn pick_emoji(&mut self, e: &str, cx: &mut Context<Self>) {
        match self.picker.take() {
            Some(Picker::React(m)) => {
                let emoji = Emoji {
                    id: None,
                    name: Some(e.to_string()),
                };
                self.toggle_reaction(&m, &emoji, false, cx);
            }
            Some(Picker::Composer) => self.insert(e),
            None => {}
        }
        cx.notify();
    }

    pub fn pick_files(&mut self, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Envoyer".into()),
        });
        cx.spawn(async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await {
                this.update(cx, |this, cx| this.upload(paths, cx)).ok();
            }
        })
        .detach();
    }

    pub fn upload(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        let (auth, text) = (self.auth.clone(), self.input.trim().to_string());
        self.clear_input();
        self.status = "Envoi du fichier…".into();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::upload(&auth, &channel.id, &text, &paths) })
                .await;
            this.update(cx, |this, cx| {
                this.status = match res {
                    Ok(()) => String::new(),
                    Err(e) => format!("Envoi impossible : {e}"),
                };
                this.last_msg_id = None;
                this.refresh_messages(cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn open_dm_with(&mut self, user: &User, cx: &mut Context<Self>) {
        let (auth, id) = (self.auth.clone(), user.id.clone());
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::open_dm(&auth, &id) })
                .await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(c) => {
                        if !this.dms.iter().any(|d| d.id == c.id) {
                            this.dms.insert(0, c.clone());
                        }
                        this.select_channel(c, cx);
                    }
                    Err(e) => this.status = format!("Conversation impossible : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn ack_current(&mut self, cx: &mut Context<Self>) {
        if self.auth.starts_with("Bot ") {
            return;
        }
        let (Some(c), Some(last)) = (self.channel.clone(), self.messages.last()) else {
            return;
        };
        let (auth, mid) = (self.auth.clone(), last.id.clone());
        cx.background_spawn(async move {
            let _ = api::ack(&auth, &c.id, &mid);
        })
        .detach();
    }

    fn send_typing(&mut self, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        if !self.logged_in || self.editing.is_some() {
            return;
        }
        if self
            .last_typing_sent
            .is_some_and(|t| t.elapsed() < TYPING_TTL)
        {
            return;
        }
        self.last_typing_sent = Some(Instant::now());
        let auth = self.auth.clone();
        cx.background_spawn(async move {
            let _ = api::typing(&auth, &channel.id);
        })
        .detach();
    }

    fn submit(&mut self, cx: &mut Context<Self>) {
        let text = self.input.trim().to_string();
        if !self.logged_in {
            return self.submit_login(cx);
        }
        if text.is_empty() {
            return;
        }
        if self.channel.is_none() {
            return;
        }
        let mut text = text;
        if self.editing.is_none() && text.starts_with('/') {
            self.clear_input();
            match self.handle_slash(&text, cx) {
                Some(t) if !t.is_empty() => {
                    self.set_input(t);
                    text = self.input.clone();
                    self.clear_input();
                }
                _ => return cx.notify(),
            }
        }
        let Some(channel) = self.channel.clone() else {
            return;
        };
        if channel.is_forum() {
            let (title, body) = match text.split_once('\n') {
                Some((t, b)) if !b.trim().is_empty() => {
                    (t.trim().to_string(), b.trim().to_string())
                }
                _ => (text.lines().next().unwrap_or("").to_string(), text.clone()),
            };
            self.clear_input();
            let auth = self.auth.clone();
            cx.spawn(async move |this, cx| {
                let res = cx
                    .background_spawn(
                        async move { api::forum_post(&auth, &channel.id, &title, &body) },
                    )
                    .await;
                this.update(cx, |this, cx| {
                    match res {
                        Ok(t) => {
                            this.threads.push(t.clone());
                            this.select_channel(t, cx);
                        }
                        Err(e) => this.status = format!("Publication impossible : {e}"),
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
            return cx.notify();
        }
        let auth = self.auth.clone();
        let editing = self.editing.take();
        let reply = self.replying.take().map(|m| m.id);
        self.clear_input();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    match editing {
                        Some(id) => api::edit(&auth, &channel.id, &id, &text),
                        None => api::send(&auth, &channel.id, &text, reply.as_deref()),
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    if e.contains("(403)") {
                        if let Some(c) = &this.channel {
                            this.no_send.insert(c.id.clone());
                        }
                    }
                    this.status = format!("Échec de l'envoi : {e}");
                }
                // Always jump to our own message.
                this.last_msg_id = None;
                this.refresh_messages(cx);
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    // ---- message actions -----------------------------------------------

    fn edit_last_own(&mut self) {
        let me = self.me.as_ref().map(|u| u.id.clone());
        if let Some(m) = self
            .messages
            .iter()
            .rev()
            .find(|m| Some(&m.author.id) == me.as_ref())
        {
            let (id, content) = (m.id.clone(), m.content.clone());
            self.editing = Some(id);
            self.replying = None;
            self.set_input(content);
        }
    }

    pub fn start_reply(&mut self, m: &Message, cx: &mut Context<Self>) {
        self.editing = None;
        self.replying = Some(m.clone());
        cx.notify();
    }

    pub fn start_edit(&mut self, m: &Message, cx: &mut Context<Self>) {
        self.replying = None;
        self.editing = Some(m.id.clone());
        self.set_input(m.content.clone());
        cx.notify();
    }

    /// First click arms the button, second click deletes.
    pub fn delete_message(&mut self, m: &Message, cx: &mut Context<Self>) {
        if self.confirm_delete.as_deref() != Some(&m.id) {
            self.confirm_delete = Some(m.id.clone());
            cx.notify();
            return;
        }
        self.confirm_delete = None;
        let Some(channel) = self.channel.clone() else {
            return;
        };
        let (auth, id) = (self.auth.clone(), m.id.clone());
        self.messages.retain(|x| x.id != id);
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::delete(&auth, &channel.id, &id) })
                .await;
            this.update(cx, |this, cx| {
                if let Err(e) = res {
                    this.status = format!("Suppression impossible : {e}");
                    this.refresh_messages(cx);
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub fn toggle_reaction(
        &mut self,
        m: &Message,
        emoji: &Emoji,
        me: bool,
        cx: &mut Context<Self>,
    ) {
        let (auth, mid, cid, emoji) = (
            self.auth.clone(),
            m.id.clone(),
            m.channel_id.clone(),
            emoji.clone(),
        );
        let cid = if cid.is_empty() {
            self.channel
                .as_ref()
                .map(|c| c.id.clone())
                .unwrap_or_default()
        } else {
            cid
        };
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::react(&auth, &cid, &mid, &emoji, !me) })
                .await;
            this.update(cx, |this, cx| {
                match res {
                    Ok(()) => this.refresh_messages(cx),
                    Err(e) => this.status = format!("Réaction impossible : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn load_older(&mut self, cx: &mut Context<Self>) {
        let (Some(channel), Some(first)) = (self.channel.clone(), self.messages.first()) else {
            return;
        };
        if self.loading_older {
            return;
        }
        self.loading_older = true;
        let (auth, before) = (self.auth.clone(), first.id.clone());
        cx.spawn(async move |this, cx| {
            let cid = channel.id.clone();
            let res = cx
                .background_spawn(async move { api::messages(&auth, &cid, Some(&before)) })
                .await;
            this.update(cx, |this, cx| {
                this.loading_older = false;
                if this.channel.as_ref().map(|c| &c.id) != Some(&channel.id) {
                    return;
                }
                match res {
                    Ok(mut older) => {
                        if this.messages.len() + older.len() > MAX_MESSAGES {
                            this.status = "Limite d'historique en mémoire atteinte.".into();
                            this.history_done = true;
                        }
                        this.history_done = older.len() < 50;
                        this.learn_users(&older);
                        older.append(&mut this.messages);
                        this.messages = older;
                    }
                    Err(e) => this.status = format!("Historique indisponible : {e}"),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    // ---- login / gateway -----------------------------------------------

    fn login(&mut self, token: String, cx: &mut Context<Self>) {
        self.login_busy = true;
        self.status = "Connexion…".into();
        cx.notify();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move {
                    let (auth, me) = api::login(&token)?;
                    let guilds = api::guilds(&auth)?;
                    let dms = api::dms(&auth).unwrap_or_default();
                    let friends = api::friends(&auth).unwrap_or_default();
                    Ok::<_, String>((auth, me, guilds, dms, friends))
                })
                .await;
            this.update(cx, |this, cx| {
                this.login_busy = false;
                match res {
                    Ok((auth, me, guilds, dms, friends)) => {
                        store::save(&auth);
                        this.auth = auth;
                        this.known_users
                            .insert(me.id.clone(), me.display_name().to_string());
                        this.me = Some(me);
                        this.guilds = guilds;
                        this.learn_dm_users(&dms);
                        this.dms = dms;
                        this.friends = friends;
                        this.logged_in = true;
                        this.autologin = false;
                        this.status.clear();
                        this.form = Default::default();
                        this.clear_input();
                        this.qr_stop.store(true, Ordering::Relaxed);
                        this.start_gateway(cx);
                        this.start_polling(cx);
                    }
                    Err(e) => {
                        if this.autologin {
                            // Stored token is stale: forget it and offer the normal login.
                            store::clear();
                            this.autologin = false;
                            this.start_qr(cx);
                        }
                        this.status = format!("Connexion impossible : {e}");
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn logout(&mut self, cx: &mut Context<Self>) {
        self.gateway_stop.store(true, Ordering::Relaxed);
        self.gateway_stop = Arc::new(AtomicBool::new(false));
        self.auth.clear();
        self.me = None;
        self.logged_in = false;
        self.connected = false;
        self.guilds.clear();
        self.dms.clear();
        self.channels.clear();
        self.messages.clear();
        self.guild = None;
        self.channel = None;
        self.cancel_compose();
        self.clear_input();
        self.status.clear();
        self.leave_voice(cx);
        self.unread.clear();
        self.members.clear();
        self.gateway_cmd = None;
        self.settings = false;
        self.profile = None;
        self.search = None;
        self.friends.clear();
        self.picker = None;
        self.switcher = None;
        self.login_mode = LoginMode::Credentials;
        self.field = Field::Email;
        store::clear();
        self.start_qr(cx);
        cx.notify();
    }

    fn start_gateway(&mut self, cx: &mut Context<Self>) {
        let (tx, rx) = flume::unbounded();
        self.gateway_cmd = Some(gateway::spawn(
            self.auth.clone(),
            tx,
            self.gateway_stop.clone(),
        ));
        cx.spawn(async move |this, cx| {
            while let Ok(ev) = rx.recv_async().await {
                if this.update(cx, |this, cx| this.on_gateway(ev, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    fn on_gateway(&mut self, ev: gateway::Event, cx: &mut Context<Self>) {
        use gateway::Event::*;
        match ev {
            Connected(up) => {
                self.connected = up;
                if up {
                    self.last_member_req = None;
                    self.request_members(cx);
                }
            }
            Members { guild_id, rows } => {
                if self.guild.as_deref() == Some(&guild_id) {
                    for r in &rows {
                        if let gateway::MemberRow::Member {
                            user,
                            status,
                            roles,
                        } = r
                        {
                            self.member_roles.insert(user.id.clone(), roles.clone());
                            self.users.insert(user.id.clone(), user.clone());
                            self.presence
                                .entry(user.id.clone())
                                .or_insert_with(|| status.clone());
                        }
                    }
                    self.members = rows;
                }
            }
            Roles(list) => {
                for (gid, roles) in list {
                    self.all_roles.insert(gid, roles.into_iter().collect());
                }
                if let Some(g) = &self.guild {
                    if let Some(r) = self.all_roles.get(g) {
                        self.roles = r.clone();
                    }
                }
            }
            Session(id) => self.session_id = Some(id),
            VoiceStates(list) => {
                for (g, c, u) in list {
                    self.voice_states.insert(u, (Some(g), c, false));
                }
            }
            VoiceStateUpdate {
                guild_id,
                channel_id,
                user_id,
                session_id,
                user,
                mute,
                ..
            } => {
                if let Some(u) = user {
                    self.users.insert(u.id.clone(), u);
                }
                let mine = self.me.as_ref().is_some_and(|m| m.id == user_id);
                match channel_id {
                    Some(c) => {
                        self.voice_states.insert(user_id, (guild_id, c, mute));
                        if mine && self.voice_target.is_some() {
                            self.voice_session = Some(session_id);
                            self.try_start_voice(cx);
                        }
                    }
                    None => {
                        self.voice_states.remove(&user_id);
                        if mine && self.voice_target.is_some() && self.voice_connected {
                            // Kicked / moved out of the channel.
                            self.voice_target = None;
                            self.voice = None;
                            self.voice_connected = false;
                            self.voice_status = "Déconnecté du vocal".into();
                        }
                    }
                }
            }
            VoiceServer {
                token, endpoint, ..
            } => {
                if let (Some(ep), true) = (endpoint, self.voice_target.is_some()) {
                    self.voice = None;
                    self.voice_server = Some((token, ep));
                    self.try_start_voice(cx);
                }
            }
            Presences(list) => self.presence.extend(list),
            Presence { user_id, status } => {
                self.presence.insert(user_id, status);
            }
            ThreadsChanged { guild_id } => {
                if self.guild.as_deref() == Some(&guild_id) {
                    self.refresh_threads(cx);
                }
            }
            MembersStale { guild_id } => {
                if self.guild.as_deref() == Some(&guild_id) {
                    self.request_members(cx);
                }
            }
            MessageCreate(m) => {
                self.learn_users(std::slice::from_ref(&m));
                self.typing
                    .remove(&(m.channel_id.clone(), m.author.id.clone()));
                let mine = self.me.as_ref().is_some_and(|u| u.id == m.author.id);
                if let Some(member) = &m.member {
                    self.member_roles
                        .insert(m.author.id.clone(), member.roles.clone());
                }
                let viewing = self.channel.as_ref().map(|c| &c.id) == Some(&m.channel_id);
                if !mine && self.notifications && (!viewing || !self.window_active) {
                    let me = self.me.as_ref().map(|u| u.id.clone()).unwrap_or_default();
                    let pinged = m.guild_id.is_none()
                        || m.mention_everyone
                        || m.mentions.iter().any(|u| u.id == me);
                    if pinged {
                        let place = match &m.guild_id {
                            None => "message privé".to_string(),
                            Some(g) => self
                                .guilds
                                .iter()
                                .find(|x| &x.id == g)
                                .map(|x| x.name.clone())
                                .unwrap_or_default(),
                        };
                        let body = if m.content.is_empty() {
                            "(pièce jointe)".to_string()
                        } else {
                            m.content.clone()
                        };
                        notify::show(&format!("{} · {place}", m.author.display_name()), &body);
                    }
                }
                if self.channel.as_ref().map(|c| &c.id) == Some(&m.channel_id) {
                    if !self.messages.iter().any(|x| x.id == m.id) {
                        self.follow_if_at_bottom();
                        self.last_msg_id = Some(m.id.clone());
                        self.messages.push(m);
                        self.ack_current(cx);
                    }
                } else if !mine {
                    let me = self.me.as_ref().map(|u| u.id.clone()).unwrap_or_default();
                    let pinged = m.guild_id.is_none()
                        || m.mention_everyone
                        || m.mentions.iter().any(|u| u.id == me);
                    let e = self
                        .unread
                        .entry(m.channel_id.clone())
                        .or_insert_with(|| Unread {
                            guild: m.guild_id.clone(),
                            mentions: 0,
                            first_id: Some(m.id.clone()),
                        });
                    if pinged {
                        e.mentions += 1;
                    }
                    if self.guild.is_none() {
                        self.refresh_dms(cx);
                    }
                }
            }
            ChannelChanged(cid) => {
                if self.channel.as_ref().map(|c| &c.id) == Some(&cid) {
                    self.refresh_messages(cx);
                }
            }
            MessageDelete { channel_id, id } => {
                if self.channel.as_ref().map(|c| &c.id) == Some(&channel_id) {
                    self.messages.retain(|m| m.id != id);
                }
            }
            Typing {
                channel_id,
                user_id,
            } => {
                if self.me.as_ref().map(|u| &u.id) != Some(&user_id) {
                    self.typing.insert((channel_id, user_id), Instant::now());
                }
            }
        }
        cx.notify();
    }

    fn follow_if_at_bottom(&mut self) {
        let off = self.scroll.offset().y;
        if off <= -self.scroll.max_offset().height + px(80.) {
            self.scroll.scroll_to_bottom();
        }
    }

    /// REST polling is only a fallback while the gateway is down.
    fn start_polling(&mut self, cx: &mut Context<Self>) {
        let stop = self.gateway_stop.clone();
        cx.spawn(async move |this, cx| loop {
            cx.background_executor().timer(POLL_FAST).await;
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let alive = this.update(cx, |this, cx| {
                if !this.connected {
                    this.refresh_messages(cx);
                    if this.guild.is_none() {
                        this.refresh_dms(cx);
                    }
                }
                // Expire typing indicators and re-render.
                this.typing.retain(|_, t| t.elapsed() < TYPING_TTL);
                cx.notify();
            });
            if alive.is_err() {
                break;
            }
        })
        .detach();
    }

    // ---- data loading --------------------------------------------------

    fn learn_users(&mut self, msgs: &[Message]) {
        for m in msgs {
            self.known_users
                .insert(m.author.id.clone(), m.author.display_name().to_string());
            self.users.insert(m.author.id.clone(), m.author.clone());
            for u in &m.mentions {
                self.known_users
                    .insert(u.id.clone(), u.display_name().to_string());
                self.users.insert(u.id.clone(), u.clone());
            }
            if let Some(r) = &m.referenced_message {
                self.users.insert(r.author.id.clone(), r.author.clone());
            }
        }
    }

    fn learn_dm_users(&mut self, dms: &[Channel]) {
        for r in dms.iter().flat_map(|c| &c.recipients) {
            self.known_users
                .insert(r.id.clone(), r.display_name().to_string());
            self.users.insert(r.id.clone(), r.clone());
        }
    }

    fn open_home(&mut self, cx: &mut Context<Self>) {
        self.guild = None;
        self.reset_channel();
        self.channels.clear();
        self.refresh_dms(cx);
        let auth = self.auth.clone();
        cx.spawn(async move |this, cx| {
            if let Ok(f) = cx
                .background_spawn(async move { api::friends(&auth) })
                .await
            {
                this.update(cx, |this, cx| {
                    this.friends = f;
                    cx.notify();
                })
                .ok();
            }
        })
        .detach();
        cx.notify();
    }

    fn reset_channel(&mut self) {
        self.channel = None;
        self.messages.clear();
        self.last_msg_id = None;
        self.history_done = false;
        self.cancel_compose();
    }

    fn refresh_dms(&mut self, cx: &mut Context<Self>) {
        let auth = self.auth.clone();
        cx.spawn(async move |this, cx| {
            let res = cx.background_spawn(async move { api::dms(&auth) }).await;
            this.update(cx, |this, cx| {
                if let Ok(dms) = res {
                    this.learn_dm_users(&dms);
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
        self.members.clear();
        self.threads.clear();
        self.forum_archived.clear();
        self.search_results.clear();
        self.pins.clear();
        if self.commands.len() > 16 {
            self.commands.clear();
        }
        if self.users.len() > 5000 {
            self.users.clear();
            self.known_users.clear();
        }
        self.messages.shrink_to_fit();
        self.roles = self.all_roles.get(&id).cloned().unwrap_or_default();
        self.member_roles.clear();
        if self.roles.is_empty() {
            self.fetch_roles(id.clone(), cx);
        }
        self.reset_channel();
        let auth = self.auth.clone();
        if !self.guild_emojis.contains_key(&id) {
            let (a, g) = (auth.clone(), id.clone());
            cx.spawn(async move |this, cx| {
                let g2 = g.clone();
                if let Ok(e) = cx
                    .background_spawn(async move { api::guild_emojis(&a, &g2) })
                    .await
                {
                    this.update(cx, |this, cx| {
                        this.guild_emojis.insert(g, e);
                        cx.notify();
                    })
                    .ok();
                }
            })
            .detach();
        }
        cx.spawn(async move |this, cx| {
            let gid = id.clone();
            let res = cx
                .background_spawn(async move { api::channels(&auth, &gid) })
                .await;
            this.update(cx, |this, cx| {
                if this.guild.as_deref() != Some(&id) {
                    return;
                }
                match res {
                    Ok(c) => {
                        this.channels = c;
                        this.refresh_threads(cx);
                        let pending = this
                            .pending_channel
                            .take()
                            .and_then(|id| this.channels.iter().find(|c| c.id == id).cloned());
                        if let Some(c) = pending {
                            this.select_channel(c, cx);
                        } else if let Some(first) = this
                            .channels
                            .iter()
                            .find(|c| !c.is_forum() && c.kind != 4)
                            .cloned()
                        {
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
        self.reset_channel();
        self.new_since = self.unread.remove(&channel.id).and_then(|u| u.first_id);
        self.nav_open = false;
        self.channel = Some(channel);
        self.status.clear();
        if let Some(c) = self.channel.as_ref().filter(|c| c.is_forum()).cloned() {
            self.load_archived(c.id, cx);
        }
        self.search = None;
        self.search_results.clear();
        self.search_note.clear();
        self.refresh_messages(cx);
        if self.side == Side::Pins && self.show_side {
            self.open_pins(cx);
        }
        self.last_member_req = None;
        self.request_members(cx);
        cx.notify();
    }

    fn refresh_messages(&mut self, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
        if channel.is_forum() || self.forbidden.contains(&channel.id) {
            return;
        }
        if self.messages.is_empty() {
            self.loading_messages = true;
        }
        let auth = self.auth.clone();
        let id = channel.id.clone();
        cx.spawn(async move |this, cx| {
            let res = cx
                .background_spawn(async move { api::messages(&auth, &id, None) })
                .await;
            this.update(cx, |this, cx| {
                // Ignore stale responses for a channel we've since left.
                if this.channel.as_ref().map(|c| &c.id) != Some(&channel.id) {
                    return;
                }
                match res {
                    Ok(fresh) => this.merge_messages(fresh, cx),
                    Err(e) if e.contains("(403)") => {
                        this.forbidden.insert(channel.id.clone());
                        this.messages.clear();
                        this.status.clear();
                    }
                    Err(e) => this.status = format!("Messages indisponibles : {e}"),
                }
                this.loading_messages = false;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Replaces the newest page, keeping any older history already loaded.
    fn merge_messages(&mut self, fresh: Vec<Message>, cx: &mut Context<Self>) {
        self.learn_users(&fresh);
        let newest = fresh.last().map(|x| x.id.clone());
        if newest != self.last_msg_id {
            if self.last_msg_id.is_none() {
                self.scroll.scroll_to_bottom();
            } else {
                self.follow_if_at_bottom();
            }
            self.last_msg_id = newest;
        }
        let cut = fresh.first().map(|m| snowflake(&m.id)).unwrap_or(0);
        let mut merged: Vec<Message> = self
            .messages
            .drain(..)
            .filter(|m| snowflake(&m.id) < cut)
            .collect();
        merged.extend(fresh);
        self.messages = merged;
        self.status.clear();
        self.ack_current(cx);
    }

    // ---- images ----------------------------------------------------------

    /// Starts downloads for any image the current view wants but doesn't have yet.
    fn ensure_images(&mut self, cx: &mut Context<Self>) {
        if !self.logged_in {
            return;
        }
        let mut wanted: Vec<String> = Vec::new();
        wanted.extend(self.me.iter().filter_map(|u| u.avatar_url()));
        wanted.extend(self.guilds.iter().filter_map(|g| g.icon_url()));
        wanted.extend(
            self.dms
                .iter()
                .flat_map(|c| &c.recipients)
                .filter_map(|u| u.avatar_url()),
        );
        if self.picker.is_some() && self.picker_tab == PickerTab::Gif {
            wanted.extend(self.gifs.iter().map(|g| g.preview.clone()));
        }
        if self.picker.is_some() {
            if let Some(g) = &self.guild {
                wanted.extend(
                    self.guild_emojis
                        .get(g)
                        .into_iter()
                        .flatten()
                        .map(|e| e.url()),
                );
            }
        }
        if let Some(p) = &self.profile_data {
            wanted.extend(p.user.banner_url());
            wanted.extend(p.badges.iter().map(|b| b.url()));
            wanted.extend(p.user.avatar_url());
        }
        for m in self.messages.iter().flat_map(|m| &m.sticker_items) {
            wanted.extend(m.url());
        }
        for row in &self.members {
            if let gateway::MemberRow::Member { user, .. } = row {
                wanted.extend(user.avatar_url());
            }
        }
        // Newest first: those are the ones on screen.
        for m in self
            .messages
            .iter()
            .rev()
            .chain(&self.search_results)
            .chain(&self.pins)
        {
            wanted.extend(m.author.avatar_url());
            if let Some(r) = &m.referenced_message {
                wanted.extend(r.author.avatar_url());
            }
            wanted.extend(
                m.attachments
                    .iter()
                    .filter(|a| a.is_image())
                    .map(|a| a.preview_url()),
            );
            for e in &m.embeds {
                wanted.extend(
                    e.image
                        .iter()
                        .chain(e.thumbnail.iter())
                        .map(|i| i.preview_url()),
                );
            }
            wanted.extend(m.reactions.iter().filter_map(|r| r.emoji.url()));
        }
        // Memory: past the cap, drop decoded images the current view doesn't use
        // (they are re-downloaded on demand) and release GPUI's decoded copy too.
        if self.images.len() > MAX_CACHED_IMAGES {
            let keep: HashSet<&String> = wanted.iter().collect();
            let stale: Vec<String> = self
                .images
                .keys()
                .filter(|k| !keep.contains(k))
                .cloned()
                .collect();
            for k in stale {
                if let Some(img) = self.images.remove(&k) {
                    gpui::ImageSource::Image(img).remove_asset(cx);
                }
            }
        }
        if self.image_failed.len() > 2000 {
            self.image_failed.clear();
        }
        for url in wanted {
            if self.image_pending.len() >= MAX_INFLIGHT_IMAGES {
                break;
            }
            if self.images.contains_key(&url)
                || self.image_pending.contains(&url)
                || self.image_failed.contains(&url)
            {
                continue;
            }
            self.image_pending.insert(url.clone());
            cx.spawn(async move |this, cx| {
                let u = url.clone();
                let bytes = cx
                    .background_spawn(async move { api::fetch_image(&u) })
                    .await;
                this.update(cx, |this, cx| {
                    this.image_pending.remove(&url);
                    match bytes.ok().and_then(|b| Some((detect_format(&b)?, b))) {
                        Some((fmt, b)) => {
                            this.images.insert(url, Arc::new(Image::from_bytes(fmt, b)));
                        }
                        None => {
                            this.image_failed.insert(url);
                        }
                    }
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    fn typing_text(&self) -> Option<String> {
        let cid = &self.channel.as_ref()?.id;
        let mut names: Vec<&str> = self
            .typing
            .iter()
            .filter(|((c, _), t)| c == cid && t.elapsed() < TYPING_TTL)
            .filter_map(|((_, u), _)| self.known_users.get(u).map(|s| s.as_str()))
            .collect();
        names.sort();
        match names.as_slice() {
            [] => None,
            [a] => Some(format!("{a} est en train d'écrire…")),
            [a, b] => Some(format!("{a} et {b} sont en train d'écrire…")),
            _ => Some("Plusieurs personnes sont en train d'écrire…".into()),
        }
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
        self.window_active = window.is_window_active();
        self.width = f32::from(window.viewport_size().width);
        self.height = f32::from(window.viewport_size().height);
        if self.status != self.last_status_seen {
            self.last_status_seen = self.status.clone();
            self.status_at = Some(Instant::now());
        } else if self.logged_in
            && !self.status.is_empty()
            && self
                .status_at
                .is_some_and(|t| t.elapsed() > Duration::from_secs(8))
        {
            self.status.clear();
            self.last_status_seen.clear();
        }
        if self.width >= 760. {
            self.nav_open = false;
        }
        self.ensure_images(cx);
        let mentions: u32 = self.unread.values().map(|u| u.mentions).sum();
        let title = if self.title_badge && mentions > 0 {
            format!("({mentions}) Discord")
        } else {
            "Discord".to_string()
        };
        if title != self.last_title {
            window.set_window_title(&title);
            self.last_title = title;
        }
        self.root(cx)
    }
}

fn main() {
    if std::env::var_os("RUST_LOG").is_some() {
        env_logger::init();
    }
    Application::new()
        .with_assets(assets::Assets)
        .run(|cx: &mut App| {
            let _ = cx.text_system().add_fonts(assets::fonts());
            let bounds = Bounds::centered(None, size(px(1280.), px(780.)), cx);
            cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    window_min_size: Some(size(px(360.), px(480.))),
                    titlebar: Some(gpui::TitlebarOptions {
                        title: Some("Discord".into()),
                        appears_transparent: true,
                        traffic_light_position: Some(gpui::point(px(12.), px(11.))),
                    }),
                    ..Default::default()
                },
                |_, cx| cx.new(DiscordApp::new),
            )
            .unwrap();
            cx.activate(true);
        });
}
