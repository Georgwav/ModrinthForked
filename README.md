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
- **Host any instance as a server.** Click **Host as server**: the Minecraft version, mod loader and server-side mods are set up for you, with a console, player list, CPU and memory use and a settings page. Friends can join over the internet through your router's automatic port forwarding or a free [playit.gg](https://playit.gg) address, linked by confirming in your browser.
- **CurseForge and Feed the Beast.** A CurseForge tab next to Discover installs CurseForge mods into an instance (matching its version and mod loader) and CurseForge and Feed the Beast modpacks as new instances.
- **Worlds and server packs:** copy or move worlds between instances, and export an instance as a ready-to-run server pack.
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

Needs Node.js, pnpm, Rust and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). The CurseForge tab needs a [CurseForge API key](https://console.curseforge.com/) in the `CURSEFORGE_API_KEY` environment variable (at build time, or when running); without one only Feed the Beast works there. `main` is protected, so open a pull request from a fork.

## Code signing policy

Windows releases are built by GitHub Actions from this repository. Only builds of `main` are signed.

- **Committers and reviewers:** [Georgwav](https://github.com/Georgwav)
- **Approvers:** [Georgwav](https://github.com/Georgwav)

Changes to `main` only land through pull requests.

**Privacy:** Threadrinth connects to Modrinth (mods and modpacks), Mojang and Microsoft (sign-in, game files, skins) and GitHub (app updates) to work, and to CurseForge, Feed the Beast and playit.gg only when you use those features. Modrinth's anonymous usage statistics are off by default and can be turned on in Settings → Privacy. Threadrinth itself collects nothing.

## License

GPL-3.0 like upstream; other packages keep their own licenses (see [COPYING.md](COPYING.md)). Modrinth's branding has been removed. The Threadrinth logo is original artwork.
