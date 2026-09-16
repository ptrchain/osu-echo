# osu-localserver (Rust)

A high-performance, standalone local osu! private server written in Rust.

`osu-localserver` allows you to experience a full Bancho-like environment on your own machine—with score submission, global & personal leaderboards, real-time performance point (PP) calculations, multi-profile management, and custom/speed-modified map support (such as osu!trainer)—without requiring Python, external web servers (Nginx/Apache), or complicated certificate setups.

---

## ✨ Features

- **Blazing Fast Native Async Server**: Powered by [Tokio](https://tokio.rs/) and [Hyper 1.0](https://hyper.rs/) for high throughput and minimal memory usage.
- **Embedded Database**: Bundled SQLite database via `rusqlite` for zero-configuration, robust, and portable storage of player profiles, best scores, play history, and beatmap caches.
- **Accurate Real-time PP Calculation**: Integrates [`rosu-pp`](https://github.com/MaxOhn/rosu-pp) to calculate performance points on the fly for standard plays, unranked maps, and custom rates.
- **Native Bancho Packet Engine**: Complete binary protocol handling for Cho/Bancho packets (login, user status, user stats, spectator, in-game chat commands, and channel messaging).
- **Automated Score Decoding**: Implements Rijndael AES decryption and LZMA replay stream parsing to extract score data, replay frames, and statistics directly.
- **Built-in TLS / HTTPS**: Automatic self-signed TLS certificate generation via `rcgen` and `tokio-rustls`, enabling secure HTTPS communication on port 443 with zero external tools.
- **osu!direct & Mirrors**: Full search and download support with automatic fallback to popular beatmap mirrors (Mishotsuna / Nerinyan / osu!direct).
- **Multi-Profile System**: Easily switch between or create multiple local profiles simply by logging in with different credentials.
- **Custom Mods & Rates**: Supports funorange osu!trainer, unranked maps, custom speed multipliers, and mod-specific leaderboards.

---

## 🚀 Getting Started

### Prerequisites

- [Rust](https://www.rust-lang.org/tools/install) (1.75 or newer recommended)
- osu! client (Stable / Fallback / Cutting Edge)

### Installation & Build

1. **Clone the repository**:
   ```bash
   git clone https://github.com/your-username/osu-localserver-rust.git
   cd osu-localserver-rust
   ```

2. **Configure environment variables**:
   Copy the example environment configuration:
   ```bash
   cp .env.example .env
   ```
   Edit `.env` to configure your settings (see [Configuration](#-configuration) below).

3. **Build and run**:
   ```bash
   # Debug mode
   cargo run

   # Optimized Release build
   cargo run --release
   ```

---

## ⚙️ Configuration

Configuration is managed via the `.env` file or environment variables:

| Variable | Default | Description |
|---|---|---|
| `SERVER_HOST` | `127.0.0.1` | Network interface to bind HTTP server. |
| `SERVER_PORT` | `5000` | Port for the HTTP server. |
| `OSU_USERNAME` | *(empty)* | Your official osu! username (used for osu!direct search integration). |
| `OSU_PASSWORD_HASH` | *(empty)* | MD5 hash of your official osu! password for osu!direct authentication. |
| `OSU_API_KEY` | *(empty)* | Optional osu! v1 API key to fetch official beatmap metadata and leaderboards. |
| `OSU_DAILY_API_KEY` | *(empty)* | Optional osu!daily API key for additional PP calculations and metadata. |

---

## 🎮 Connecting to the Server

You can connect your osu! client to the local server using either of the following methods:

### Method 1: `-devserver` Flag (Recommended)

Add the `-devserver` argument to your osu! shortcut:

1. Create a shortcut to `osu!.exe`.
2. Right-click the shortcut and select **Properties**.
3. In the **Target** field, append:
   ```
   -devserver 127.0.0.1:5000
   ```
4. Click **OK** and launch osu! from the shortcut.

### Method 2: Hosts File Redirection

If using HTTPS (port 443 with self-signed certificate):

1. Run the server with administrator privileges so it can bind to port `443` and generate certificates in `.data/local-tls/`.
2. Add the following entries to your `hosts` file (`C:\Windows\System32\drivers\etc\hosts` on Windows, or `/etc/hosts` on Linux/macOS):
   ```
   127.0.0.1 osu.ppy.sh
   127.0.0.1 c.ppy.sh
   127.0.0.1 c1.ppy.sh
   127.0.0.1 c2.ppy.sh
   127.0.0.1 c3.ppy.sh
   127.0.0.1 c4.ppy.sh
   127.0.0.1 c5.ppy.sh
   127.0.0.1 c6.ppy.sh
   127.0.0.1 ce.ppy.sh
   127.0.0.1 a.ppy.sh
   ```
3. Import the generated root CA certificate from `.data/local-tls/` into your operating system's trusted root certificate store.

---

## 💬 In-Game Chat Commands

When connected to the server, you can execute commands in `#osu` or via the osu!direct search bar:

- `!help`: Display the list of available commands.
- `!stats`: View your current profile statistics (PP, accuracy, ranked score, play count).
- `!recent` / `!r`: View your most recent submitted score and PP.
- `!best`: Display your top plays.
- `!mode <mod>`: Switch active leaderboard filtering (e.g. `!mode hd`, `!mode hr`, `!mode dt`).
- `!leaderboard <type>`: Toggle between PP leaderboard and score leaderboard.

---

## 🧪 Testing & Code Quality

Run tests:
```bash
cargo test
```

Run clippy:
```bash
cargo clippy -- -D warnings
```

Format code:
```bash
cargo fmt --check
```

---

## 📂 Project Structure

```
├── src/
│   ├── main.rs            # Application entry point, server startup & background tasks
│   ├── packets.rs         # Bancho packet definitions & serializers
│   ├── utils.rs           # Utility helpers (math, hashing, formatting, logging)
│   ├── derpy_hooves.rs    # osu! beatmap & score signature verification
│   ├── db/                # SQLite database queries, migrations, & schema
│   ├── handlers/          # HTTP request handlers (web, score decode, score submit, cho)
│   ├── server/            # HTTP/HTTPS routing, multipart parser, response & TLS
│   └── types/             # Domain models (Score, Beatmap, Player, Mods, Leaderboard, Config)
├── .data/                 # Local SQLite database & TLS certificates (git-ignored)
├── .env.example           # Template for environment configuration
├── Cargo.toml             # Package metadata and dependencies
└── Cargo.lock             # Exact dependency graph for reproducible builds
```

---

## 📄 License

This project is licensed under the [MIT License](LICENSE).
