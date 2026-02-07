<div align="center">

# 🧠 Brain.fm Discord Presence

**Share your focus sessions on Discord**

[![macOS](https://img.shields.io/badge/platform-macOS-000000?style=flat-square&logo=apple&logoColor=white)](https://github.com/yourusername/brainfm-presence)
[![Rust](https://img.shields.io/badge/rust-1.70+-orange?style=flat-square&logo=rust&logoColor=white)](https://rustup.rs)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue?style=flat-square)](LICENSE)

<img src="https://cdn.brain.fm/images/focus/focus_mental_state_bg_small_aura.webp" alt="Brain.fm Presence" width="400">

*A lightweight menu bar app that displays your Brain.fm session as Discord Rich Presence*

</div>

---

## ✨ Features

- 🎯 **Zero configuration** — Works out of the box
- 🖥️ **Menu bar integration** — Runs quietly, no windows needed
- 🎵 **Dynamic presence** — Shows mode, track, neural effect & duration
- 🖼️ **Album art** — Full track artwork support
- 🔄 **Auto-reconnect** — Handles Discord restarts gracefully
- ⚡ **Smart sync** — Uses direct API for 100% accuracy, falls back to offline cache

---

## 🚀 Quick Start

### Requirements
- [Brain.fm Desktop App](https://brain.fm) (run at least once)
- [Discord](https://discord.com) running

### Install

#### Option 1: Download (Recommended)

1. Download the latest `.dmg` from [**Releases**](../../releases)
2. Open the image and drag the app to `Applications`
3. Launch the app (right-click → Open on first run)

> 💡 **That's it!** The app will appear in your menu bar.

<details>
<summary><strong>Option 2: Build from Source</strong></summary>

```bash
# Clone
git clone https://github.com/yourusername/brainfm-discord.git
cd brainfm-discord

# Build & Run
cargo run --release
```

Requires [Rust 1.70+](https://rustup.rs)

</details>

---

## ⚠️ Platform Support

| Platform | Status | Notes |
|----------|--------|-------|
| **macOS** | ✅ Supported | Fully tested and working |
| **Windows** | 🚧 Not Yet | Contributions welcome! |
| **Linux** | ❌ Not Planned | Brain.fm desktop not available |

### 🤝 Help Wanted: Windows Support

We'd love to support Windows, but we need help testing and implementing it!  
**If you're a Windows user and Rust developer**, please check out the [contribution guidelines](#contributing).

---

## 🛠️ Development

```bash
# Dev build
cargo build

# Release build
cargo build --release

# Create macOS .app bundle
cargo install cargo-bundle
cargo bundle --release

# Create .dmg installer (requires: brew install create-dmg)
create-dmg \
  --volname "Brain.fm Presence" \
  --window-size 600 400 \
  --icon-size 128 \
  --app-drop-link 450 200 \
  "Brain.fm Presence.dmg" \
  "target/release/bundle/osx/Brain.fm Presence.app"
```

---



---

## 🔧 Troubleshooting

<details>
<summary><strong>Discord presence not showing?</strong></summary>

- Ensure Discord is running
- Check **Settings → Activity Privacy → Activity Status** is enabled
- App retries connection every 60s
</details>

<details>
<summary><strong>Brain.fm state not detected?</strong></summary>

- Run Brain.fm desktop app at least once
- Make sure music is playing
- Updates may take ~15 seconds
</details>

---

## 🤝 Contributing

Contributions are welcome! Whether it's:

- 🐛 Bug reports
- 💡 Feature requests  
- 🪟 **Windows support** (especially needed!)
- 📖 Documentation improvements

Please open an issue or submit a pull request.

---

## 📄 License

[MIT License](LICENSE) — Use freely, attribution appreciated.

---

<div align="center">

Made with ❤️ for Brain.fm enthusiasts

*Focus better. Share proudly.*

</div>
