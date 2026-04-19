# Gemini-Extended

Gemini-Extended is a native, hardware-accelerated Linux desktop application for Google's Gemini models, built with Rust, GTK4, and libadwaita.

Unlike typical Electron or web-wrapper AI clients, Gemini-Extended contains **zero web technologies (No HTML, CSS, JavaScript, or NodeJS runtime bloat)**. It leverages the official `gemini-cli` acting as an autonomous backend engine, communicating via a headless JSON stream.

## Features
- **Native GTK4 UI:** Integrates perfectly into modern Linux desktop environments (GNOME/CachyOS).
- **Blazing Fast:** Written entirely in Rust with async Tokio.
- **Zero NPM/Web Bloat:** Uses `WebKitGTK`? No. Uses Electron? No. Pure native UI rendering.
- **Agentic Capabilities:** Inherits the local file system access and bash command execution of the `gemini-cli`.

## Architecture
1. **Frontend:** `gtk4-rs` and `libadwaita-rs`
2. **Middleware:** `tokio` (Async runtime) + inter-process channels (`glib`).
3. **Engine:** Exceutes `gemini -p "<prompt>" -o stream-json` in the background as a subprocess.

## Prerequisites
- Rust and Cargo (`paru -S rust`)
- GTK4 and Libadwaita development libraries (`paru -S gtk4 libadwaita`)
- The official Gemini CLI installed locally (`npm install -g @google/gemini-cli`)

## Build
```bash
cargo build --release
```

## License
MIT
