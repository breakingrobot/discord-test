# discord-test

A minimal Discord client in Rust using [GPUI](https://crates.io/crates/gpui).

- Log in with a **bot token** (paste it at the prompt, or set `DISCORD_TOKEN`).
- Browse servers and text channels, read the last 50 messages, send messages.
- Messages refresh by polling every 3s (REST only; no gateway websocket).

```
DISCORD_TOKEN=... cargo run --release
```

Requires GPUI's Linux system deps (wayland/xcb, vulkan, fontconfig, etc.).
`Cargo.lock` pins `libc` to 0.2.189 because `xattr` 0.2.3 (via gpui) fails with 0.2.190.
Don't use user tokens: self-botting violates Discord's ToS.
