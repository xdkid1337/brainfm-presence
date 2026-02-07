<div align="center">

# 🧠 Brain.fm Presence

**Show your Brain.fm focus sessions on Discord**

<a href="https://github.com/xdkid1337/brainfm-presence/releases"><img src="https://cdn.simpleicons.org/apple/999999" alt="macOS" height="28"></a>
[![Rust](https://img.shields.io/badge/Rust_1.80+-f74c00?style=for-the-badge&logo=rust&logoColor=white)](https://rustup.rs)
[![License: MIT](https://img.shields.io/badge/MIT-3da639?style=for-the-badge)](LICENSE)
[![v1.2.0](https://img.shields.io/badge/v1.2.0-8b5cf6?style=for-the-badge)](https://github.com/xdkid1337/brainfm-presence/releases/tag/v1.2.0)

<br>

*A lightweight macOS menu bar app that displays your Brain.fm session as Discord Rich Presence — zero configuration, just install and focus.*

</div>

---

## ✨ Features

| | |
|---|---|
| 🎯 **Zero Config** | Works out of the box — no tokens, no setup |
| 🖥️ **Menu Bar** | Runs silently in the macOS menu bar |
| 🎵 **Rich Presence** | Mode, track name, genre, neural effect & elapsed time |
| 🖼️ **Album Art** | Full CDN artwork for every track |
| ⚡ **Smart Sync** | Direct API + offline cache fallback for 100% accuracy |
| 🔄 **Auto-Reconnect** | Handles Discord restarts with exponential backoff |
| 🧠 **LRU Cache** | Bounded in-memory cache — safe for long sessions |

---

## 🚀 Install

### Download (Recommended)

1. Grab **Brain.fm Presence.dmg** from [**Releases**](https://github.com/xdkid1337/brainfm-presence/releases)
2. Open the image → drag to **Applications**
3. Right-click → **Open** on first launch (macOS Gatekeeper)

> 💡 The app appears in your menu bar. That's it.

### Requirements

- macOS 12+
- [Brain.fm Desktop App](https://brain.fm) (launched at least once)
- [Discord](https://discord.com) running

<details>
<summary><strong>Build from Source</strong></summary>

```bash
git clone https://github.com/xdkid1337/brainfm-presence.git
cd brainfm-presence

# Run directly
cargo run --release --bin brainfm-presence

# Or create .app bundle + .dmg
cargo install cargo-bundle
cargo bundle --release --bin brainfm-presence

brew install create-dmg
create-dmg \
  --volname "Brain.fm Presence" \
  --window-size 600 400 \
  --icon-size 128 \
  --app-drop-link 450 200 \
  "Brain.fm Presence.dmg" \
  "target/release/bundle/osx/Brain.fm Presence.app"
```

Requires [Rust 1.80+](https://rustup.rs)

</details>

---

## 🔧 Troubleshooting

<details>
<summary><strong>Discord presence not showing?</strong></summary>

- Ensure Discord is running
- Check **Settings → Activity Privacy → Activity Status** is enabled
- The app retries the connection automatically with backoff

</details>

<details>
<summary><strong>Brain.fm state not detected?</strong></summary>

- Launch the Brain.fm desktop app at least once
- Start playing music — detection takes ~15 seconds on first sync

</details>

---

## 🤝 Contributing

Contributions welcome — bug reports, feature ideas, or pull requests.

---

## 📄 License

[MIT](LICENSE)

---

<div align="center">

Made with ❤️ for Brain.fm enthusiasts

*Focus better. Share proudly.*

</div>
