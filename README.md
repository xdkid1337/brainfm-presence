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

### Homebrew (Recommended)

```bash
brew install --cask xdkid1337/tap/brainfm-presence
```

### Manual Download

1. Grab **Brain.fm-Presence.dmg** from [**Releases**](https://github.com/xdkid1337/brainfm-presence/releases)
2. Open the DMG → drag **Brain.fm Presence** to **Applications**
3. **Remove the quarantine flag** before first launch:
   ```bash
   xattr -cr "/Applications/Brain.fm Presence.app"
   ```
4. Open the app — it appears in your menu bar. That's it.

<details>
<summary><strong>Alternative: System Settings method</strong></summary>

If you prefer not to use Terminal:

1. Try opening the app — macOS will block it
2. Go to **System Settings → Privacy & Security**
3. Scroll down and click **"Open Anyway"** next to the blocked app
4. Authenticate with your password

> ⚠️ On macOS Sequoia 15.1+, the "Open Anyway" button may not appear. Use the `xattr -cr` command above instead.

</details>

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
<summary><strong>macOS says "cannot verify" or blocks the app?</strong></summary>

This app is not notarized with Apple (it's open-source and free). Run this in Terminal to allow it:

```bash
xattr -cr "/Applications/Brain.fm Presence.app"
```

If you installed via Homebrew, this is handled automatically.

</details>

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
