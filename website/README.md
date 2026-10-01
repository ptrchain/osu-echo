# Local profile website

Start `cargo run` as usual, then open http://127.0.0.1:5000/.
Static assets can be edited directly in `website/static/` during development. No Node build or additional service is required.

The profile interface uses the official osu!web stylesheet, Torus fonts, default cover and grade icons, with local HTML/JavaScript adaptations. Official navigation links open osu.ppy.sh in another tab.

## Login and profiles

- Sign in with a local username. Names have the same case-sensitive identity as game login; surrounding whitespace is removed.
- A new username creates and saves a profile using the game's profile initializer. An existing username selects its existing scores.
- Website login selects the active player in the running game server.
- There are no local passwords. Do not enter your official osu! password. This retains the existing local server's profile selection model.
- Viewing `/users/<username>` is read-only and does not select that profile. `/u/2` resolves to the active profile for links using the game's fixed user ID.
- Browser sessions last one day or until the server restarts. Website sign-out clears the browser identity, not the ongoing game session.
- Sign in, sign out, and settings changes require a same-origin request and a session CSRF token. The server binds to 127.0.0.1.

## Data and current limits

- Best performance and accuracy match the game's deduplication/weighting rules. Top and recent lists support standard, relax and autopilot; the selector changes the viewed scores, not the game's mod configuration.
- Lists expose up to 100 scores, initially five, with expandable details and Show more.
- Historical includes monthly counts derived from stored plays, most-played maps and Recent Plays (24h).
- The active player's osu!daily rank estimate is shown when `OSU_DAILY_API_KEY` is configured.
- Country rank compares your local pp against the official osu!standard country leaderboard when `OSU_CLIENT_ID` and `OSU_CLIENT_SECRET` are configured.
- Sign in and choose Edit profile to rename your profile or upload/reset its picture. Renames reject collisions, preserve scores and update the active player and browser sessions.
- Uploaded PNG/JPEG/GIF/WebP files are verified, limited to 2 MB and 4096 pixels per side.
- 1-click official osu! profile import can import your avatar, me! bio, country, location, and playstyle devices directly from osu.ppy.sh.

## Upstream assets

See `static/vendor/NOTICE.txt` and `static/vendor/LICENCE.txt`. osu!web is copyright ppy Pty Ltd and contributors, licensed under AGPL-3.0-or-later. The local website adaptations are provided under the same licence.
