# osu-echo

[![Discord](https://img.shields.io/badge/Discord-Join%20Community-5865F2?logo=discord&logoColor=white)](https://discord.gg/Jpdq6sS3Kn)

A local osu! server written in Rust. It lets you run your own private server on your computer for score saving, performance points (PP) calculation, local leaderboards, and beatmap downloads.

## Features

- **Score & Replay Tracking**: Automatically saves your plays, scores, and `.osr` replay files locally in a self-contained SQLite database.
- **Real-Time PP Calculation**: Computes live PP for all plays using `rosu-pp` across all game modes (Standard, Taiko, Catch, Mania).
- **Custom & Unranked Maps**: Full support for unranked maps, practice diffs, custom speed rates, and osu!trainer modifications.
- **Built-in osu!direct Mirror**: In-game beatmap search and 1-click downloads powered by the Catboy (Mino) mirror (no osu! supporter or external accounts required).
- **In-Game Bot Companions**:
  - **BanchoBot**: Manages leaderboard statuses, personal bests, score announcements, and profile statistics.
  - **Tillerino**: Send `/np` in chat or PM to get instant difficulty breakdowns and PP calculations for Nomod, HD, HR, DT, etc.
- **Local Leaderboards & Profiles**: Track personal bests, total score, hit accuracy, play count, and global rank estimates.
- **Discord Webhook Integration**: Automatically post submitted plays with Rich Embeds and configurable minimum PP thresholds directly to a Discord channel.
- **Streamlined Setup**: Quick interactive setup with auto-detection. External API keys (osu! v1 and osudaily) power online leaderboards and global rank calculations.

## Requirements

- **osu! client (Stable)**
- *Optional (only needed if compiling from source)*: **Rust (Cargo)**

## Getting Started

### 1. Get the Server

#### Option A: Download Pre-built Release (Recommended)
1. Download the latest `osu-echo-vX.X.X-windows-x86_64.zip` from [Releases](https://github.com/ptrchain/local-bancho-rust/releases).
2. Extract the zip into a folder.
3. Run `osu-echo.exe`.

#### Option B: Build from Source
```bash
git clone https://github.com/ptrchain/local-bancho-rust.git
cd local-bancho-rust
cargo run --release
```

### 2. First Launch & Setup

When you start `osu-echo` for the first time, an interactive **Configuration Wizard** runs automatically in the terminal, asking which setup experience you prefer:

- **[1] Quick Setup (Recommended)**: Auto-detects your osu! installation, prompts for essential API keys (osu! v1 & osudaily) for online leaderboards and global rank calculation, automatically sets up and trusts the local HTTPS certificate, and applies recommended defaults.
- **[2] Advanced Setup**: Granular control for power users—customize directory paths (Songs/Replays/Screenshots), server host IP and port settings, essential API keys (osu! v1 API & osudaily ranking), leaderboard scoring modes, profile country flag, and official Bancho account sync.

> [!NOTE]
> **Privacy & Local Storage**: All API keys and account credentials entered during setup or configured in `.env` are stored strictly locally on your machine and are never transmitted to any third-party or remote server. Passwords for account sync are MD5-hashed before saving.

> [!TIP]
> In **Quick Setup**, default values (such as detected osu! path and recommended settings) can be accepted instantly by pressing **Enter**.

On future launches, the server will start immediately using your saved configuration. If you ever want to re-run the setup wizard later, pass the `--setup` (or `-s`) flag:
```bash
osu-echo.exe --setup
```
Alternatively, you can manually edit `.env` at any time.

### 3. Connect from osu!

To connect your osu! client to the local server:

1. Right-click your `osu!.exe` shortcut and select **Properties**.
2. In the **Target** field, add `-devserver localhost` to the end.
   Example:
   ```text
   "C:\Games\osu!\osu!.exe" -devserver localhost
   ```
3. Launch osu! using the shortcut.
4. Log in with any username and password. The server will create your profile automatically.

## In-Game Commands

You can send commands in chat channels or via private message to **BanchoBot** or **Tillerino**.

### BanchoBot Commands

| Command | Description |
|---|---|
| `!help` | List available commands |
| `!stats` / `!profile` | Show your performance stats, rank, and play count |
| `!recent` / `!r` | Show your most recent play with PP and accuracy |
| `!tops` / `!t` | Show your top 5 highest PP plays |
| `!mybest` / `!pb` | Show your best score on the current beatmap |
| `!leaderboard` / `!lb` | Show top local scores on the current beatmap |
| `!mode <std/taiko/ctb/mania>` | Switch active game mode |
| `!country <code>` | Set country flag on your profile (e.g. `!country US`, `!country DE`) |
| `!status <ranked/unranked/loved>` | Change the status of the current beatmap |
| `!friend <add/remove/list/sync>` | Manage friends or sync with official Bancho |
| `!recentfeed <on/off>` | Toggle live score announcements in the `#recent` channel |
| `!recalc` / `!recalculate` | Recalculate profile PP and stats from stored scores |
| `!restrictself [reason]` / `!unrestrict` | Simulate official Bancho ban / account restriction |
| `!avatar <url or path>` | Change your in-game profile avatar |
| `!config` | Show server settings and active configuration |
| `!roll [max]` | Roll a random number (default 1-100) |
| `!wipe` | Wipe profile stats and play count |
| `!clearscores` / `!clearmap` | Clear your scores on the current beatmap or /np selection |

### Tillerino Commands

Send commands via PM to **Tillerino** or type `/np` in any channel:

| Command | Description |
|---|---|
| `/np` | Show current song info, star rating, and 95%–100% PP breakdown |
| `!with <mods>` | Calculate PP with specific mods (e.g. `!with HDHR`, `!with DT`) |
| `!acc <value>` | Calculate exact PP for a custom accuracy (e.g. `!acc 98.5`) |
| `!r` / `!recommend` | Recommend a beatmap to play |

## Command-Line Options

```text
Usage: osu-echo [OPTIONS]

Options:
  -h, --help           Print help information
  -v, --version        Print version information
  -d, --debug          Enable verbose debug logging (or hold Shift when starting)
  -s, --setup          Run or re-run the interactive setup wizard
      --reconfigure    Alias for --setup
      --trust-cert     Install and trust the local TLS certificate in Windows Root store
```

## Data & Backups

All server data is stored locally in the `.data/` directory next to the executable:
- `server.db`: SQLite database holding user profiles, scores, beatmap cache, and stats.
- `replays/`: Saved `.osr` replay files.
- `avatars/`: User avatars.

To back up your progress or move to another machine, simply copy the `.data/` folder.

## Community & Support

Have questions, suggestions, or need help? Join our [Discord Server](https://discord.gg/Jpdq6sS3Kn)!

## Credits & Acknowledgements

Special thanks to:
- [local-osu-server](https://github.com/jeevanjohnson/local-osu-server) by [Jeevan Johnson](https://github.com/jeevanjohnson) and its contributors for the original inspiration and foundational work that helped make this project possible.

## License

This project is licensed under the MIT License.
unofficial / not affiliated with ppy
