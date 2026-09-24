# Roadmap

## Done

- Prism-style instance folders (`instance.cfg`) with a library refresh button.
- Windows/Linux database compatibility.
- Threadrinth branding, amber default and accent color picker.
- Upstream sync script (`scripts/sync-upstream.sh`).

## 1. Updates

- **App updater**: publish signed Threadrinth releases on GitHub and point the in-app updater at them (our own signing key; Modrinth's updater is currently disabled).
- **Automatic upstream sync**: a scheduled workflow that runs `scripts/sync-upstream.sh` on each new Modrinth release and opens a PR, so CI checks it before it lands.
- Rebuild the release workflow (removed with the other Modrinth-only workflows).

## 2. More modpack sources

- CurseForge and Feed The Beast modpacks, behind a settings toggle.

## 3. Appearance

- Two new themes.
