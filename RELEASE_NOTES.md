# osu-echo v1.1.0

## Highlights & Improvements

- **1:1 osu!web Local Profile & Userpage Interface**:
  - Replaced the placeholder status page with an authentic 1:1 replica of the official osu!web user profile page running directly on the Bancho HTTP/HTTPS server (port 5000 / 443). Accessible at `/`, `/profile`, `/u/2`, or `/users/<username>`.
  - In-game avatar and profile card clicks (`/u/2`, `/users/<name>`) now seamlessly open the local osu!web profile in your default browser rather than redirecting to external sites.
  - Zero external folder dependency: all HTML, CSS, JavaScript, fonts, and vendor assets are compiled directly into the standalone binary using `rust-embed`, while automatically honoring local `./website/static/` disk overrides if present.
  - Comprehensive multi-mode profile statistics across Standard (`osu`), Taiko, Catch, and Mania with Relax (`rx`) and AutoPilot (`ap`) filtering.
  - Real-time play count graphs, historical performance curves, top 100 plays, recent plays, and most played beatmaps.
  - Official PP exponential decay formula ($\sum \text{pp}_i \cdot 0.95^i$) and weighted hit accuracy calculation.

- **Official 352-Medal Achievement System**:
  - Complete 352-medal official catalog loaded directly from `data/medals.json`, aligning 100% with official osu! `achievement_id`s, descriptions, and artwork slugs.
  - Real-time submission evaluation engine: automatically awards combo medals (500x to 2000x), Star Pass & FC medals (1★ to 10★) computed with live mod-adjusted star ratings via `rosu-pp`, mod introductions (HD, HR, DT, FL, etc.), and secret hush-hush achievements.
  - Live in-game unlock feedback: broadcasts in-game client notification toasts, BanchoBot chat announcements in `#osu`, and passes `achievements-new` in ranking charts.
  - Full web medals gallery with official categories (Skill & Dedication, Hush-Hush, Mod Introduction, Beatmap Packs), official SVG badges streaming from CDN, unlock timestamps, and rich hover tooltips.
  - Added in-game `!medals` and `!medal <search>` BanchoBot chat commands.

- **Web-Based Bancho Score & Replay Importer**:
  - Integrated score ingestion engine: imports top plays, first-place scores, and leaderboards directly from official osu! profiles (`osu.ppy.sh/users/<id>/scores/<type>`) with zero API keys required.
  - Automatic username-to-numeric-ID resolution, with osu! API v1 and OAuth API v2 fallbacks when configured.
  - Automated `.osu` beatmap downloading from public mirrors and `.osr` replay extraction (via `osu_session` cookie or API v1).
  - Configurable conflict resolution: `skip`, `replace_if_better`, or `overwrite`.
  - Real-time web UI import panel with live progress bar, stage indicator, and counters, alongside the in-game `!importscores` BanchoBot command.
  - Automatic retroactive medal evaluation and profile PP/accuracy recalculation upon import.

- **1:1 osu!web Account Settings & BBCode Signature Editor**:
  - Modernized account settings into the authentic osu!web `/settings` and `/home/account/edit` page layout with dedicated sidebar navigation tabs.
  - Interactive BBCode toolbar (`[b]`, `[i]`, `[strike]`, `[heading]`, `[url]`, `[quote]`, `[list]`, `[img]`, `[size]`) with instant live preview for custom `me!` userpage signatures.
  - Extended profile customization: country flags, location, interests, occupation, website, and social links (Twitter, Discord).
  - Playstyle input device selectors and drag-and-drop / ordered profile section management.

- **Score Management & Deletion Engine**:
  - Web score deletion via `POST /site/scores/delete` with SQLite safety archive (`deleted_scores`) to prevent accidental data loss.
  - Live in-game stats recalculation: automatically syncs updated PP, accuracy, and rank via `user_presence` and `user_stats` packets to the client.

---

# osu-echo v1.0.6

## Highlights & Improvements

- **Redesigned Setup Experience (Quick vs. Advanced)**:
  - First-time startup and the `--setup` wizard now provide an intuitive setup selection:
    - **[1] Quick Setup (Recommended)**: Auto-detects your osu! folder, guides you through essential API keys, installs & trusts the local HTTPS certificate, and applies recommended defaults automatically.
    - **[2] Advanced Setup**: Gives power users complete control over custom folder paths (Songs, Replays, Screenshots), server bind IP and port, essential API keys, leaderboard scoring mode (Score vs. PP), profile country flags, and Bancho account sync.
  - Added `-s` / `--setup` and `--reconfigure` command-line flags to easily re-run the wizard at any time.

- **Magenta ASCII Logo & Smart Screen Clearing**:
  - The wizard now features the branded magenta ASCII logo on every step with smart console clearing (`cls` / ANSI purge), delivering a clean, clutter-free terminal experience.
  - Added natural pauses after local certificate generation and hosts configuration so results can be reviewed before moving to the connection guide.

- **Essential API Keys & Critical Consequence Warning**:
  - Reclassified **osu! Legacy v1 API** and **osudaily API** keys as essential for authentic Bancho server functionality (online beatmap leaderboards, global score comparisons, and real-time PP rank calculation).
  - Added concise step-by-step guidance on how to obtain keys directly within the wizard.
  - Implemented a prominent, high-visibility critical warning banner detailing the exact gameplay consequences if keys are omitted, with an intuitive default-reconfigure retry flow.

- **Discord Webhook Score Integration**:
  - Automatically posts submitted scores directly to a Discord channel via webhook using Discord Rich Embeds with grade-themed side colors, player profile, beatmap title, star difficulty, score, combo, accuracy, and PP.
  - Configurable via `DISCORD_WEBHOOK_URL` with an optional minimum PP threshold (`DISCORD_WEBHOOK_MIN_PP`) in `.env`.

- **In-Game Score Management (`!clearscores` / `!clearmap`)**:
  - Added `!clearscores` (aliases: `!clearmap`, `!removescores`, `!deletescores`) to wipe your scores on the currently selected map or recent `/np` beatmap.
  - Automatically recalculates profile PP, weighted accuracy, and stats, pushing a live HUD update packet (`ChoUserStats`) directly to the client.

- **Client Submission Hang Fix on Failed Plays**:
  - Fixed an issue where the osu! client would hang indefinitely on *"Submitting score..."* after failing a song.
  - The server now constructs and returns valid beatmap and overall ranking charts while avoiding popup notifications and keeping unpassed plays out of the ranked leaderboard database, correctly updating play counts and user stats.

- **Minimal Default Logging & Shift-Startup Debug Mode**:
  - Cleaned up default console output for a sleek, minimal, high-performance terminal experience.
  - Added `-d` / `--debug` command-line flag and Windows Shift-key detection on launch to easily enable verbose packet and route debugging.

- **Hardened Beatmap Ranking & Diff Selection Safeguards**:
  - Prevented ranking commands (`!rank`, `!love`, `!unrank`) and `/np` difficulty switching from accidentally mutating canonical beatmap sets or hijacking top diff IDs for custom practice maps.

- **Privacy & Local Storage Transparency**:
  - Added explicit privacy notices throughout the wizard, `.env.example`, and documentation confirming that all API keys, usernames, and passwords (stored as one-way MD5 hashes) remain strictly local on your machine and are never transmitted to third-party servers.

---

# osu-echo v1.0.5

## Highlights & Improvements

- **Official Weighted Overall Accuracy**:
  - Implemented the official osu! exponential decay weighting formula ($\sum \text{acc}_i \cdot 0.95^i / \sum 0.95^i$) across all top scores on a player's profile.
  - Overall accuracy is now computed in tandem with total weighted PP in a single sorted pass, replacing the unweighted arithmetic mean and properly rewarding high-accuracy top plays.
  - Updated the in-game `!recalculate` (and `!recalc`) BanchoBot summary report to explicitly format and display weighted accuracy alongside total PP (`PP: Xpp | Acc: Y.YY%`).

- **Hardened Beatmap Isolation & Stolen ID Defense**:
  - Added strict canonical hash validation for score submissions: if a submitted beatmap references an official `beatmap_id`, its file hash is verified against the canonical ranked database entry.
  - Mismatched diffs (e.g. locally modified or rate-changed `.osu` files that retain the original map's `BeatmapID` header) are instantly stripped of online IDs (`beatmap_id: 0`) and unranked, preventing custom or practice plays from scoring on official leaderboards or distorting profile stats.
  - Expanded heuristic detection during local `.osu` parsing to detect rate edits (e.g. `1.1x`, `1.2x`, `0.85x`), practice diffs (`prac`), cuts, buffs, and nerfs, unconditionally preventing cloned diffs from inheriting parent IDs upon import.

- **Score Submission Deadlock Resolution**:
  - Resolved an asynchronous self-deadlock in native score submission (`process_native_submission`) occurring when a submitted replay's player name does not match the logged-in player (e.g. watching external replays or multi-profile setups).
  - Explicitly drops active state read locks before acquiring write locks for client notification queueing, ensuring the server stays completely responsive.

- **Seamless Database Migration & Self-Healing (No Reset Required)**:
  - Users do **not** need to wipe or start with a fresh database.
  - Startup migration automatically sanitizes legacy SQLite databases: any corrupt custom, rate, or practice diffs that previously borrowed or collided with official IDs are demoted (`beatmap_id: 0`, `approved: 0`).
  - Running `!recalculate` in-game immediately re-indexes profile stats using the new weighted accuracy engine without losing play history, local scores, or configurations.

- **Simulated Bancho Restriction (`!restrictself` / `!unrestrict`)**:
  - Added a playful `!restrictself [reason]` command (alias `!restrict`) that accurately mimics an official osu! server ban / account restriction.
  - Pops the official in-game yellow toast notification banner: *"Your account is currently in restricted mode! Please visit the osu! website for more information."*
  - Automatically sends the authentic BanchoBot direct message (with optional custom reason), revokes Bancho privileges, wipes in-game rank (`#0`) and PP in the client panel, disables public channel chatting, and returns `error: ban` on score submissions.
  - Accurately mirrors official Bancho behavior by preserving the player's personal best banner in song select, allowing restricted players to still view their own best records locally.
  - Running `!restrictself` again (or `!unrestrict` / `!unrestrictself` / `!restrictself off`) seamlessly lifts the restriction, restores privileges, recalculates profile stats, and pushes an unban welcome notification.

- **Non-Blocking Leaderboard & Replay Network Concurrency**:
  - Eliminated global `AppState` lock contention during official Bancho score lookups and `.osr` replay streaming.
  - HTTP requests to official servers are now executed asynchronously without holding global read or write locks, ensuring packet routing, chat, and concurrent client requests remain silky smooth even under slow network conditions.

- **Instant Pre-Login Song Select Responsiveness**:
  - Optimized the pre-login leaderboard wait loop, reducing polling intervals from 100ms down to 15ms with a tighter bounded timeout.
  - Completely eliminates client UI lag and audio micro-stutters when entering song select immediately upon launching osu! before the login handshake finishes.

- **Personal Best Banner PP Isolation**:
  - Fixed a score formatting bug where `show_pp_for_personal_best = true` would inadvertently format PP into all leaderboard list entries even when `pp_leaderboard = false`.
  - PP values are now strictly isolated to the personal best banner, keeping the main leaderboard list in pure raw score mode as configured.

---

# osu-echo v1.0.4

## Highlights & Improvements

- **In-Game Profile PP Recalculation (`!recalculate` / `!recalc`)**:
  - Added the `!recalculate` (alias `!recalc`) BanchoBot chat command, executable in public channels (e.g. `#osu`) or direct messages.
  - Recalculates PP and accuracy across all recorded scores using the latest `rosu-pp` calculation engine.
  - Automatically updates player profile total PP and weighted overall accuracy, syncs global rank via osu!daily if configured, and immediately broadcasts updated stats packets to the client with a detailed summary report.

- **Multi-Difficulty Beatmapset Disambiguation & PP Fix (#B0001)**:
  - Fixed a collision bug in local song folder scans and API fallbacks where mapsets with multiple difficulties or practice diffs could match the wrong `.osu` file or inherit top difficulty attributes, resulting in distorted PP values and practice diffs ranking the top diff.
  - Implemented strict MD5 hash verification on all local and API `.osu` file retrievals, preventing newer or updated beatmap versions from calculating PP against outdated/nerfed local files.
  - Removed set-ID API hijacking on unknown hashes, ensuring unranked practice and custom difficulties retain their true unranked status (`approved: 0`), independent `beatmap_id`, and accurate local attributes.
  - Enabled direct ranking of practice and unindexed difficulties by MD5 without modifying or colliding with parent ranked diffs.
  - Cleaned up stale `/np` and user status packet resolution so browsing unindexed or practice diffs no longer resolves or carries forward top difficulty metadata.
  - *Special thanks to kaan for reporting!*

- **Full Beatmapset Ranking & Status Management (`!rank set` / `!rank diff`)**:
  - Extended `!rank`, `!love`, and `!unrank` commands to support ranking entire beatmapsets or individual difficulties:
    - `!rank set`: Ranks all difficulties in the currently selected mapset.
    - `!rank set <set_id>` (or `!rank <set_id> set`): Ranks all difficulties for the specified beatmapset ID (scanning local songs and fetching from API if needed).
    - `!rank diff` / `!rank` / `!rank <id>`: Ranks only the individual active or specified difficulty, preserving practice diff isolation without corrupting top difficulties.
  - Extended `!status [set/diff] [id]` to report set-wide difficulty status breakdown (Ranked, Loved, Qualified, Unranked).
  - Implemented a multi-tier fallback pipeline for incoming score submissions: if a submitted map hash isn't yet indexed in the database, it resolves the map via API -> local Songs folder scan -> active player state -> `/np` state, preventing lost score submissions.
  - Resolved an issue where running `!setstatus` or `!ranked` on unindexed local beatmaps failed silently; BanchoBot now parses the local `.osu` file, stores it in SQLite, and updates its ranked status seamlessly.

- **Profile Switching Identity Safety & F9 Region Resolution**:
  - Fixed a profile switching bug where logging into a different account could inherit a stale `pending_login_name` cached from previous leaderboard requests. Authoritative login payload bytes now always take precedence.
  - Properly handled `OsuLogout` packets to cleanly purge user sessions and active player state upon logout.
  - Fixed the F9 user panel displaying an unknown/white flag: user country codes are now persisted in SQLite, loaded immediately during the login handshake, and updated via presence broadcasts upon resolution.

---

# osu-echo v1.0.3

## Highlights & Performance Improvements

- **Instant Login & Non-Blocking Startup**:
  - Offloaded external osu! API stat lookups, osu!daily global rank updates, and friend profile syncing into non-blocking background tasks (`tokio::spawn`).
  - The login handshake now immediately serves cached friend presences and profile stats from SQLite, completely eliminating startup freezes, connection buffering, and client timeouts.
  - Relocated default avatar downloading (`a.ppy.sh`) off the main startup sequence into a non-blocking background task with a 2-second timeout, preventing startup hangs when offline.

- **Instant Leaderboard Loading & In-Memory Caching**:
  - Added an in-memory TTL cache (`bancho_score_cache`) for fetched Bancho leaderboards with automatic expiry cleanup, making map switching in song select load leaderboards instantly without repeated network roundtrips.
  - Accelerated friend leaderboard requests (`/web/osu-osz2-getscores.php`) by fetching official Bancho scores concurrently using `tokio::task::JoinSet` instead of sequentially waiting on each network request.
  - Resolved pre-login leaderboard stalls by capturing username hints from `us`/`u` parameters (`pending_login_name`) and serving beatmap metadata immediately even if song select is opened before the login handshake completes.

- **Dynamic Results Screen Map Ranking & Accurate Combo Records**:
  - Implemented `calculate_map_ranks` to accurately compute `rankBefore` and `rankAfter` on post-play score submission charts.
  - Accurately ranks plays against both local database records and online Bancho leaderboards (handling personal best improvements, top 50 positions, and leaving the rank blank when placing outside the top 50 instead of defaulting to `#1`).
  - Fixed an issue where the score chart always displayed a flashing "NEW" combo badge: submission charts now properly track the highest combo achieved across all submitted scores on the beatmap and profile, preventing false "NEW" flags when a higher combo was already achieved in a different play.

- **osu!direct Status Filtering & Pagination**:
  - Corrected search status query mapping against the Catboy mirror for Ranked/Approved (`r=0`, `r=7` → `status=1,2`), Pending/WIP (`r=2` → `status=0,-1`), Graveyard (`r=5` → `status=-2`), and Loved (`r=8` → `status=4`).
  - Direct search responses now report accurate beatmap status codes so in-game status badges (Ranked, Approved, Qualified, Loved, Pending) display correctly in the Direct browser.
  - Added pagination support via the `p` query parameter, enabling smooth browsing across multiple pages of search results.

- **Engine Upgrades, Avatar Caching & Disk Optimization**:
  - Upgraded performance calculation engine to `rosu-pp` 4.0.1 (`rosu-map` 0.2.1, `rosu-mods` 0.4.1) and updated Tillerino difficulty attribute lookups to use `.od()`.
  - Added local disk caching for downloaded player and friend avatars in `.data/avatars/` to eliminate redundant HTTP downloads.
  - Eliminated redundant full-directory disk scans in song resolution and `/np` lookups, preventing micro-stutters when browsing beatmaps.

---

# osu-echo v1.0.2

## Highlights & Bug Fixes

- **Friend Sync & Identity Overwrite Fix**:
  - Resolved an issue where Friend 2's identity and stats could be overwritten by the local player's ID or cause presence loops.
  - Automatically purges corrupted friend ID 2 / friend 2 profiles from the database on startup.
  - Properly filters out Peppy (ID 2) and the local player ID from friend presence/sync packets.
  - *Special thanks to appl for reporting!*

- **osu!direct Downloads & Preview Media**:
  - Fixed route precedence bug where in-game map download requests were swallowed by generic web prefixes, resulting in failed 0-byte responses.
  - Downloads now stream/proxy `.osz` archives directly from the Catboy mirror (with automatic Nerinyan fallback) with proper archive headers.
  - Added proxy handlers for beatmap thumbnails (`b.ppy.sh/thumb/`), audio previews (`b.ppy.sh/preview/`), and beatmap card covers (`assets.ppy.sh/beatmaps/`).

- **Tillerino Diff Matching & Mirror Access**:
  - Fixed Tillerino defaulting to DISCO★PRINCE by setting client `User-Agent: osu!` to bypass Cloudflare 403 blocks on API mirrors.
  - Fixed Tillerino selecting the top diff (e.g. Master) instead of the diff you are currently viewing (e.g. Hard) via difficulty hint matching.
  - Synchronized active beatmap selection in memory when browsing song select leaderboards.
  - Fixed local beatmap scanner from falsely matching unindexed `.osu` files.

- **Networking, TLS & Devserver Subdomains**:
  - Added `b.localhost` to self-signed TLS certificates and automatic Windows `hosts` file configuration.
  - Added an automatic loopback port 80 listener for direct client HTTP requests to localhost subdomains.
  - Removed `panic=abort` in release profile to enable stack unwinding and clean diagnostic traces.

---

# osu-echo v1.0.0

Initial release of **osu-echo**, a local osu! server written in Rust for score saving, performance points (PP) calculation, local leaderboards, and beatmap downloads.

## Highlights

- **Standalone & Portable**: Compiled with full link-time optimization (LTO) and bundled SQLite. No external runtimes or databases required.
- **Real-Time PP Calculation**: Live PP computation powered by `rosu-pp` across all game modes (Standard, Taiko, Catch, Mania).
- **Score & Replay Saving**: Automatically saves scores, statistics, and `.osr` replays to a local database.
- **Custom & Unranked Maps**: Full support for unranked maps, practice diffs, speed changes, and osu!trainer modifications.
- **Built-in osu!direct**: In-game search and 1-click downloads powered by the Catboy (Mino) mirror with no Supporter tag needed.
- **In-Game Bot Companions**:
  - **BanchoBot**: Manages leaderboard statuses, personal bests, score announcements (`#recent`), and profile settings.
  - **Tillerino**: Send `/np` in chat or PM for instant star rating and 95%–100% PP breakdowns (`!with`, `!acc`).
- **First-Launch Setup Wizard**: Automatically detects your osu! installation and guides initial setup.

## Quickstart

1. Download and extract `osu-echo-v1.0.0-windows-x86_64.zip` (or run `osu-echo.exe` directly).
2. Start `osu-echo.exe` and follow the quick setup prompts.
3. Edit your `osu!.exe` shortcut target to include `-devserver localhost`:
   ```text
   "C:\Games\osu!\osu!.exe" -devserver localhost
   ```
4. Launch osu! and log in with any username and password to create your profile.

## Community & Support

- Discord: https://discord.gg/Jpdq6sS3Kn

## Acknowledgements

Special thanks to [local-osu-server](https://github.com/jeevanjohnson/local-osu-server) by Jeevan Johnson and its contributors for the original inspiration and foundational work.
