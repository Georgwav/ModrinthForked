# Roadmap

## 1. Updates
- **App updater**: publish signed Threadrinth releases on GitHub and point the in-app updater at them (our own signing key; Modrinth's updater is currently disabled).
- **Upstream sync**: a scheduled workflow that merges new Modrinth releases into a PR, runs CI, and flags conflicts, so we stay on the newest version without breaking our changes.
- Rebuild the release workflow (removed with the other Modrinth-only workflows).

## 2. More modpack sources
- CurseForge and Feed The Beast modpacks, behind a settings toggle.

## 3. Appearance
- Accent color slider that works with every theme (e.g. OLED + red).
- Two new themes.
- Threadrinth amber as the default accent.
