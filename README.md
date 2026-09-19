# osu-localserver

A local osu! server written in Rust. It lets you run your own private server on your computer for score saving, performance points (PP) calculation, local leaderboards, and beatmap downloads.

## Features

- Saves scores and replays locally.
- Calculates PP in real time using rosu-pp.
- Supports unranked maps, custom speed rates, and osu!trainer.
- In-game beatmap search and direct downloads powered by the Catboy (Mino) mirror (no osu! supporter or external accounts required).
- Local leaderboards and profile statistics.
- Easy account setup: just log in with any username and password to create a local profile.
- Zero-configuration start: osu! credentials and API keys are 100% optional.

## Requirements

- Rust (cargo)
- osu! client (Stable)

## Getting Started

### 1. Configure (Optional)

Copy `.env.example` to `.env` if you want to change any default settings:

```bash
cp .env.example .env
```

The default settings work out of the box (runs on `127.0.0.1:5000`).

### 2. Run the Server

Start the server using cargo:

```bash
cargo run --release
```

### 3. Connect from osu!

To connect your osu! client to the local server:

1. Right-click your `osu!.exe` shortcut and select Properties.
2. In the Target field, add `-devserver localhost` to the end.
   Example:
   `"C:\Games\osu!\osu!.exe" -devserver localhost`
3. Launch osu! using the shortcut.
4. Log in with any username and password. The server will create your profile automatically.

## Chat Commands

You can send commands in chat or via private message to the bot:

- `!help` - List available commands.
- `!stats` - Show your performance stats and play count.
- `!recent` or `!r` - Show your most recent play.
- `!best` - Show your top scores.
- `/np` - Send your current playing song to get PP values.

## License

This project is licensed under the MIT License.
