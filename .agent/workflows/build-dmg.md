---
description: Build macOS .dmg installer for Brain.fm Presence
---

# Build DMG Workflow

This workflow builds a release `.dmg` installer for macOS distribution.

## Prerequisites

- Rust toolchain installed (`rustup`)
- `cargo-bundle` installed: `cargo install cargo-bundle`
- `create-dmg` installed: `brew install create-dmg`

## Steps

// turbo-all

### 1. Build release binary

```bash
cargo build --release --bin brainfm-presence
```

### 2. Create .app bundle

```bash
cargo bundle --release --bin brainfm-presence
```

This creates `target/release/bundle/osx/Brain.fm Presence.app`

### 3. Remove old DMG (if exists)

```bash
rm -f "Brain.fm Presence.dmg"
```

### 4. Create DMG with create-dmg

```bash
create-dmg \
  --volname "Brain.fm Presence" \
  --window-size 600 400 \
  --icon-size 128 \
  --app-drop-link 450 200 \
  "Brain.fm Presence.dmg" \
  "target/release/bundle/osx/Brain.fm Presence.app"
```

### 5. Verify DMG was created

```bash
ls -lh "Brain.fm Presence.dmg"
```

## Output

The final `Brain.fm Presence.dmg` will be in the project root, ready for upload to GitHub Releases.

## Optional: Custom DMG appearance

You can customize the DMG further with these `create-dmg` options:

```bash
create-dmg \
  --volname "Brain.fm Presence" \
  --volicon "assets/icon.icns" \           # Volume icon
  --background "assets/dmg-background.png" \ # Custom background image (600x400)
  --window-pos 200 120 \                   # Window position on screen
  --window-size 600 400 \                  # Window size
  --icon-size 128 \                        # Icon size
  --icon "Brain.fm Presence.app" 150 200 \ # App icon position
  --app-drop-link 450 200 \                # Applications shortcut position
  --hide-extension "Brain.fm Presence.app" \
  "Brain.fm Presence.dmg" \
  "target/release/bundle/osx/Brain.fm Presence.app"
```
