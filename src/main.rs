mod api;
mod emoji;
mod gateway;
mod qr;
mod store;
mod ui;

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

use api::{Channel, Emoji, Guild, Message, User};

/// REST polling happens only while the gateway is down.
const POLL_FAST: Duration = Duration::from_secs(4);
const TYPING_TTL: Duration = Duration::from_secs(8);
const MAX_INFLIGHT_IMAGES: usize = 8;

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
    show_members: bool,
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
            show_members: true,
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
        if self.switcher.is_some() {
            return self.on_switcher_key(ev, cx);
        }
        let len = self.input.chars().count();
        let before = self.input.len();
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
            _ => match &ks.key_char {
                Some(ch) => self.insert(ch),
                None => return,
            },
        }
        if self.input.len() > before {
            self.send_typing(cx);
        }
        cx.notify();
    }

    // ---- login form ------------------------------------------------------

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
                if let (Some(ch), Some(q)) = (&ks.key_char, &mut self.switcher) {
                    q.push_str(ch);
                    self.switcher_sel = 0;
                }
            }
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
        let Some(channel) = self.channel.clone() else {
            return;
        };
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
        self.unread.clear();
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
        gateway::spawn(self.auth.clone(), tx, self.gateway_stop.clone());
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
            Connected(up) => self.connected = up,
            MessageCreate(m) => {
                self.learn_users(std::slice::from_ref(&m));
                self.typing
                    .remove(&(m.channel_id.clone(), m.author.id.clone()));
                let mine = self.me.as_ref().is_some_and(|u| u.id == m.author.id);
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
        }
    }

    fn learn_dm_users(&mut self, dms: &[Channel]) {
        for r in dms.iter().flat_map(|c| &c.recipients) {
            self.known_users
                .insert(r.id.clone(), r.display_name().to_string());
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
        self.reset_channel();
        let auth = self.auth.clone();
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
        self.reset_channel();
        self.unread.remove(&channel.id);
        self.channel = Some(channel);
        self.status.clear();
        self.refresh_messages(cx);
        cx.notify();
    }

    fn refresh_messages(&mut self, cx: &mut Context<Self>) {
        let Some(channel) = self.channel.clone() else {
            return;
        };
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
                    Err(e) => this.status = format!("Messages indisponibles : {e}"),
                }
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
        for m in &self.messages {
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
        self.ensure_images(cx);
        self.root(cx)
    }
}

fn main() {
    Application::new().run(|cx: &mut App| {
        let bounds = Bounds::centered(None, size(px(1280.), px(780.)), cx);
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
