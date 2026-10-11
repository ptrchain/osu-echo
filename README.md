# osu-echo

[![Discord](https://img.shields.io/badge/Discord-Join%20Community-5865F2?logo=discord&logoColor=white)](https://discord.gg/Jpdq6sS3Kn)

A local osu! server written in Rust. Run a private server on your machine for score saving, PP calculation, local leaderboards, medals, and beatmap downloads.

## Features

- **Score & Replay Tracking**: Stores scores, stats, and `.osr` replays in a local SQLite database.
- **Real-Time PP Calculation**: Computes live PP using `rosu-pp` across Standard, Taiko, Catch, and Mania.
- **Web Profile**: Local osu!web player page (`http://localhost:5000/users/<username>`) with PP curves, top plays, play history, and customizable bio.
- **352 Achievement Medals**: Offline medal tracking with in-game unlock notifications and commands.
- **Official Score Importer**: Import your official osu! top plays, pinned scores, replays, and medals into your local profile.
- **In-Game Beatmap Mirror**: In-game beatmap search and 1-click downloads via the Mino mirror (no supporter needed).
- **In-Game Bots**:
  - **BanchoBot**: Leaderboard management, personal bests, score announcements, and stats lookup.
  - **Tillerino**: `/np` difficulty breakdown and PP calculation for any mod combination.
- **Web Dashboard**: Edit settings, paths, API keys, and profile customization at `http://localhost:5000/settings`.
- **Discord Webhook**: Automatically post submitted plays with embeds to Discord.
- **Custom Maps & Rates**: Full support for unranked maps, practice diffs, and custom speed rates (osu!trainer).

## Requirements

- **osu! client (Stable)**
- *Optional*: **Rust toolchain** (only needed if building from source)

## Getting Started

### 1. Download or Build

#### Option A: Pre-built Release (Recommended)
1. Download the latest `osu-echo-vX.X.X-windows-x86_64.zip` from [Releases](https://github.com/ptrchain/local-bancho-rust/releases).
2. Extract the zip into a folder.
3. Run `osu-echo.exe`.

#### Option B: Build from Source
```bash
git clone https://github.com/ptrchain/local-bancho-rust.git
cd local-bancho-rust
cargo run --release
```

### 2. Initial Setup

On first launch, an interactive setup wizard runs in your terminal:

- **Quick Setup (Recommended)**: Auto-detects your osu! folder, installs the local TLS certificate, and applies default settings.
- **Advanced Setup**: Configures directory paths, ports, external API keys (osu! v1/v2 and osudaily for global ranks), and Bancho sync.

> [!NOTE]
> All credentials and API keys are stored locally in `.env` and are never sent to external servers.

To re-run setup at any time:
```bash
osu-echo.exe --setup
```

### 3. Connect from osu!

1. Right-click your `osu!.exe` shortcut and select **Properties**.
2. Add `-devserver localhost` to the end of the **Target** field:
   ```text
   "C:\Games\osu!\osu!.exe" -devserver localhost
   ```
3. Launch osu! through the shortcut.
4. Log in with any username and password (accounts are created automatically).

### 4. Web Interface

While `osu-echo` is running, open your browser to access:
- **User Profile**: `http://localhost:5000/users/<username>`
- **Settings & Dashboard**: `http://localhost:5000/settings`

## In-Game Commands

Send commands in chat channels or via private message to **BanchoBot** or **Tillerino**.

### BanchoBot Commands

| Command | Description |
|---|---|
| `!help` | List available commands |
| `!stats` / `!profile` | Show profile stats and rank |
| `!recent` / `!r` | Show most recent score |
| `!tops` / `!t` | Show top 5 PP plays |
| `!mybest` / `!pb` | Show personal best on the current map |
| `!leaderboard` / `!lb` | Show local leaderboard for the current map |
| `!medals` / `!medal <name>` | Show medal progress or search medal details (`!medals sync` to recalculate) |
| `!importscores [username]` | Import official osu! scores, replays, and medals |
| `!mode <std/taiko/ctb/mania>` | Switch active game mode |
| `!country <code>` | Set profile country flag (e.g. `!country US`) |
| `!status <ranked/unranked/loved>` | Change beatmap leaderboard status |
| `!friend <add/remove/list/sync>` | Manage friends or sync from Bancho |
| `!recentfeed <on/off>` | Toggle score announcements in `#recent` |
| `!recalc` | Recalculate profile PP from saved scores |
| `!restrictself [reason]` / `!unrestrict` | Toggle account restriction state |
| `!avatar <url or path>` | Change profile avatar |
| `!config` | Show current server configuration |
| `!roll [max]` | Roll a random number (1–100 default) |
| `!wipe` | Reset profile stats and play count |
| `!clearscores` | Clear scores on the current beatmap |

### Tillerino Commands

Send via PM to **Tillerino** or type `/np` in any channel:

| Command | Description |
|---|---|
| `/np` | Show song info, star rating, and PP values (95%–100%) |
| `!with <mods>` | Calculate PP with specific mods (e.g. `!with HDHR`, `!with DT`) |
| `!acc <value>` | Calculate PP for custom accuracy (e.g. `!acc 98.5`) |
| `!r` / `!recommend` | Recommend a beatmap to play |

## Command-Line Options

```text
Usage: osu-echo [OPTIONS]

Options:
  -h, --help           Print help information
  -v, --version        Print version information
  -d, --debug          Enable verbose debug logging
  -s, --setup          Run or re-run the interactive setup wizard
      --trust-cert     Install and trust the local TLS certificate in Windows Root store
```

## Data & Backups

All server data is stored locally in the `.data/` directory:
- `server.db`: SQLite database (user profiles, scores, beatmap cache)
- `replays/`: Saved `.osr` replay files
- `avatars/`: Custom user avatars

To back up your data or move it to another PC, copy the `.data/` folder.

## Community & Support

Need help or want to suggest a feature? Join our [Discord Server](https://discord.gg/Jpdq6sS3Kn).

## Credits

- Inspired by [local-osu-server](https://github.com/jeevanjohnson/local-osu-server) by [Jeevan Johnson](https://github.com/jeevanjohnson).

## License

MIT License. Unofficial project; not affiliated with osu! or ppy.

