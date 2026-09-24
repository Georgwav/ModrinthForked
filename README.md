# ![Threadrinth](branding/threadrinth-wordmark.svg)

**Threadrinth** is a Minecraft launcher based on the [Modrinth App](https://github.com/modrinth/code). It works the same way, with the changes below, and it updates to each new Modrinth version automatically.

> Not affiliated with or endorsed by Modrinth / Rinth, Inc.

## What's different from the Modrinth App

- **Your instance folders are all it needs.** Put an instance folder into the instances folder, click **Refresh**, and it shows up, ready to play. The Modrinth App can lose track of instances when its internal list gets out of sync with the folders. Threadrinth reads the folders themselves, like Prism Launcher does.
- **Brings your Modrinth App instances along.** Instances made in the official Modrinth App are recognized with their Minecraft version, mod loader, icon and playtime. Nothing in the Modrinth App is changed.
- **Moving between Windows and Linux works.** Sharing one instances folder between both, or moving it from one to the other, no longer breaks the launcher.
- **Your colors.** Pick any accent color, including white, in Settings → Appearance. It works with every theme. Amber is the default.
- **Updates itself.** Threadrinth checks this repository for new versions and installs them in one click.

## Download

Get the latest version from the [releases page](https://github.com/Georgwav/Threadrinth/releases) (Windows and Linux).

## For developers

```sh
cp packages/app-lib/.env.prod packages/app-lib/.env
pnpm install
pnpm app:dev
```

You need Node.js, pnpm, Rust and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/).

- **Our code** is mostly in `packages/app-lib` (instance folders), `apps/app-frontend` (settings, colors, logo) and `branding/`. Everything else is upstream Modrinth code, kept as close to the original as possible so updates merge cleanly.
- **Releases:** run the **Release** workflow in the Actions tab. Versions follow Modrinth's (0.21.x is based on Modrinth 0.21).
- **Modrinth updates:** the **Upstream sync** workflow merges each new Modrinth release, and publishes a new release once CI passes. If the merge conflicts, it opens an issue instead. To merge by hand, run `scripts/sync-upstream.sh <tag>`.
- **Plans:** see [ROADMAP.md](ROADMAP.md).
- **Contributing:** `main` is protected, so open a pull request from a fork.

## License

GPL-3.0 like upstream; other packages keep their own licenses (see [COPYING.md](COPYING.md)). Modrinth's branding has been removed. The Threadrinth logo is original artwork.
