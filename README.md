# ![Threadrinth](branding/threadrinth-wordmark.svg)

A Minecraft launcher based on the [Modrinth App](https://github.com/modrinth/code), with a few extras. It updates to every new Modrinth version automatically.

> Not affiliated with or endorsed by Modrinth / Rinth, Inc.

<p>
  <img src="branding/screenshots/library.webp" alt="Threadrinth library with instances grouped by game version, in the Ember theme" width="49%">
  <img src="branding/screenshots/startup.webp" alt="Threadrinth starting up" width="49%">
</p>

## What's different

- **Instance folders just work.** Drop an instance folder into the instances folder and click **Refresh**. It shows up ready to play, like in Prism Launcher.
- **Brings your Modrinth App instances along** with their version, mod loader, icon and playtime.
- **Works across Windows and Linux**, even with one shared instances folder.
- **More themes and colors:** Ember, Sand, Orchid, Amethyst and Blossom, plus any accent color.
- **Skin history:** every skin you wear stays on the skins page, even if you change it on minecraft.net.
- **Updates itself** in one click.

## Install

**Windows** (PowerShell):

```powershell
irm https://raw.githubusercontent.com/Georgwav/Threadrinth/main/scripts/install.ps1 | iex
```

**Linux:**

```sh
curl -fsSL https://raw.githubusercontent.com/Georgwav/Threadrinth/main/scripts/install.sh | sh
```

Both download the latest release, check it against its published checksum and install it. Run them again at any time to reinstall. You can also download the installers from the [releases page](https://github.com/Georgwav/Threadrinth/releases).

## Development

```sh
cp packages/app-lib/.env.prod packages/app-lib/.env
pnpm install
pnpm app:dev
```

Needs Node.js, pnpm, Rust and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). `main` is protected, so open a pull request from a fork.

## License

GPL-3.0 like upstream; other packages keep their own licenses (see [COPYING.md](COPYING.md)). Modrinth's branding has been removed. The Threadrinth logo is original artwork.
