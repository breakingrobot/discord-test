# discord-test

A minimal Discord client in Rust using [GPUI](https://crates.io/crates/gpui).

- Log in with a **user or bot token** (paste it at the prompt, or set `DISCORD_TOKEN`). User tokens violate Discord's ToS; use at your own risk.
- Discord-style UI (French): server rail, categories, DMs, user panel, grouped messages with day dividers, replies, attachments, caret/paste in the composer.
- No voice chat, no member list, no embeds/images/emoji/reactions yet.
- Messages refresh by polling every 3s (REST only; no gateway websocket).

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
