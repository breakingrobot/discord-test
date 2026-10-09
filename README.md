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
- Not implemented: voice, full server member list, custom/server emoji picker, threads, search, settings,
  presence of friends, notifications.
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
