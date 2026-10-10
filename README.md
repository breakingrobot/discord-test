# discord-test

A minimal Discord client in Rust using [GPUI](https://crates.io/crates/gpui).

- **Login like the native app:** QR code (scan with the Discord mobile app), e-mail + password (with 2FA),
  or paste a token. The session is saved in the OS credential store (Windows Credential Manager /
  macOS Keychain / Linux keyutils) and reused on the next launch; "Quitter" forgets it.
  `DISCORD_TOKEN` also works. Bot tokens are accepted too.
- Discord-style UI (French): server rail with icons and unread pills / mention badges, categories, DMs, friends
  list, user panel, participants panel, grouped messages with day dividers, avatars, replies, image attachments,
  embeds, reactions, clickable links, highlighted mentions.
- Real-time via the Gateway (messages, edits, reactions, deletions, typing) with session resume and exponential
  reconnect backoff. REST polling only runs while the gateway is down.
- Behaves like the web client to avoid the spam filter: browser User-Agent, `X-Super-Properties`, API v9,
  same IDENTIFY properties, read acknowledgements.
- Reply / edit (Up arrow edits your last message) / delete (click twice) / react (picker or click a reaction) /
  copy, upload files (button or drag & drop), emoji picker, Ctrl+K quick switcher.
- Composer is a live Markdown editor: **bold**, *italic*, __underline__, ~~strike~~, `code`, links are styled as you type.
  Ctrl+B / Ctrl+I / Ctrl+U / Ctrl+E / Ctrl+Shift+X insert markers, Shift+Enter adds a new line, Ctrl+V pastes.
- Search (Ctrl+F or "Rechercher"), pinned messages, real server member list with presence and role groups
  (via the gateway's member-list subscription, user accounts only), server custom emojis in the picker,
  profile popup (click an avatar or member), settings panel, mention count in the window title.
- Threads (sidebar, "Fil" action, thread chip under messages) and forum / media channels (post list with
  archived posts, create a post from the composer: title, Shift+Enter, message).
- Friend presence (online / idle / do-not-disturb) in the friends list and DM list; role colours on names.
- Desktop notifications for mentions and DMs when the window is unfocused or the channel is not open
  (PowerShell toast on Windows, `notify-send` on Linux, `osascript` on macOS; toggle in Réglages).
- Slash commands: client-side `/shrug /tableflip /unflip /me /spoiler`, plus bot application commands via
  autocomplete (string / integer / number / boolean options; no sub-commands or user/channel options).
- GIF picker (Discord's Tenor proxy) and a light theme (Réglages); preferences are saved in the config dir.
- Full profiles (`GET /users/{id}/profile`): banner / accent colour, badges, pronouns, bio, account and server
  join dates, guild roles, Nitro / boost dates, mutual servers and friends, connected accounts, bot and guild-tag chips.
- Messages: highlighted user / role / channel mentions and @everyone / @here (clickable), `<t:…>` timestamps,
  reply headers with connector line (deleted originals handled), system messages (join, pin, boost, thread created),
  stickers. Roles are read from READY (REST fallback).
- 2025-style design: Light / Ash / Dark / Onyx themes, top title bar, squircle server rail, rounded channel
  list, floating user panel, resizable sidebar, Lucide icons (ISC) and the Inter font (OFL) embedded, tooltips,
  fade-in popovers, density setting (compact / default / spacious), redesigned settings, friends tabs.
- Mentions inbox, status picker (online / idle / dnd / invisible), "NOUVEAUX" unread divider, click a reply to
  jump to the original message.
- Low memory: decoded-image cache capped and evicted (GPU copies released too), per-guild caches dropped on
  switch, bounded history, small CDN sizes, release profile with LTO + strip.
- Responsive layout (side panel becomes an overlay under 1050 px; under 760 px the sidebar slides over the chat via ☰),
  skeleton placeholders while avatars / images / messages / channels load, a "jump to latest" button, and clear
  messages for missing permissions (HTTP 403: locked channel view, disabled composer).
- Not implemented: voice, private threads, desktop notification sounds, sub-commands, GIF favourites.
  Several of these features (member list, presence, slash commands, GIF search, QR login) rely on
  undocumented endpoints reproduced from community docs and are untested here.
- Using a user account with a third-party client is against Discord's Terms of Service (account risk is yours).
  QR and password login follow community-documented protocols and have not been verified against Discord here.

```
DISCORD_TOKEN=... cargo run --release
```

Requires GPUI's Linux system deps (wayland/xcb, vulkan, fontconfig, etc.).
`Cargo.lock` pins `libc` to 0.2.189 because `xattr` 0.2.3 (via gpui) fails with 0.2.190.


## Windows x64

Built by `.github/workflows/windows.yml` on a Windows runner (GPUI compiles its
DirectX shaders with `fxc.exe` at build time, so it can't be cross-compiled from Linux).
Download `discord-test-windows-x64` from the workflow run's artifacts, or build locally on
Windows with `cargo build --release`.
