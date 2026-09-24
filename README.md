# ![Threadrinth](branding/threadrinth-wordmark.svg)

**Threadrinth** is a Minecraft launcher forked from the [Modrinth App](https://github.com/modrinth/code). It's the same launcher with a few changes on top, and it stays up to date with upstream Modrinth.

> Not affiliated with or endorsed by Modrinth / Rinth, Inc.

## What's different

- **Folder-based instances, like Prism Launcher.** Each instance folder has an `instance.cfg`. On startup, or when you click **Refresh** in the library, the launcher scans the instances folder. Any folder it doesn't know yet is imported, and renamed folders are picked up. Nothing is ever deleted.
- **Works across Windows and Linux.** A database written by a Windows build opens fine on Linux and the other way around. If a database is truly incompatible, it's kept as a backup and your instances come back from their `instance.cfg` files.
- **Your colors.** Amber by default, with an accent color picker (Settings → Appearance) that works with every theme.
- **Separate from the official app.** Threadrinth has its own data folder and updates itself from this repository's releases.

See [ROADMAP.md](ROADMAP.md) for what's next.

## How the repository is laid out

```
apps/
  app/                  Desktop shell (Tauri, Rust): window, installers, Tauri commands
  app-frontend/         Launcher UI (Vue 3): pages, settings, library
packages/
  app-lib/              Launcher core (Rust): instances, downloads, launching, app.db
  ui/                   Shared Vue components and layouts
  assets/               Styles, theme variables and icons
  api-client/           Client for the Modrinth API
  utils/                Older shared TypeScript helpers
  blog/                 Changelog shown in the app
  tooling-config/       ESLint, Prettier, TypeScript and Tailwind config
  daedalus/             Minecraft and mod loader version metadata (Rust)
  ariadne/              Shared IDs and networking types (Rust)
  modrinth-content-management/  Modpack install and diff logic (Rust)
  async-minecraft-ping/, path-util/, serde-binhum/  Small Rust helpers
branding/               Threadrinth logo sources and icon generator
scripts/                Upstream sync, i18n and changelog tooling
.github/                CI workflows, issue templates, code owners
```

**Our code** is mostly in `packages/app-lib` (instance scanning, `instance.cfg`, database fixes), `apps/app-frontend` (accent color, refresh button, logo) and `branding/`. Everything else is upstream Modrinth code, kept as close to the original as possible so updates merge cleanly.

## Development

```sh
cp packages/app-lib/.env.prod packages/app-lib/.env
pnpm install
pnpm app:dev
```

Requires Node.js, pnpm, Rust and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Run `branding/generate-assets.sh` after changing the logo.

## Releases and updates

Download Threadrinth from the [releases page](https://github.com/Georgwav/Threadrinth/releases). Installed copies update themselves from there.

- **Releasing:** run the **Release** workflow (Actions tab), or push a `v*` tag. It builds signed Windows and Linux installers and publishes them with the manifest the in-app updater reads.
- **Versions:** Threadrinth follows the major.minor of the Modrinth version it's based on (`scripts/upstream-version.txt`) and counts its own patches, e.g. 0.21.0, 0.21.1, then 0.22.0 once Modrinth 0.22 is merged.

## Staying on the newest Modrinth version

This happens automatically. Every day the **Upstream sync** workflow checks for a new Modrinth App release, merges it into a pull request, and once CI passes, merges that and publishes a new Threadrinth release. If the merge conflicts with our changes, it opens an issue instead.

To merge by hand:

```sh
scripts/sync-upstream.sh v0.21.6    # a Modrinth release tag, or leave empty for upstream/main
```

The script deletes again everything listed in [`scripts/upstream-removed.txt`](scripts/upstream-removed.txt) (the website, API server and other parts we don't ship), so those never cause conflicts, and prints the remaining steps.

### Repository secrets

| Secret                                                            | Used for                                                                                                                   |
| ----------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------- |
| `TAURI_SIGNING_PRIVATE_KEY`, `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | Signing releases so the in-app updater accepts them                                                                        |
| `THREADRINTH_BOT_TOKEN`                                           | Upstream sync: a fine-grained token with Contents, Pull requests, Issues and Workflows (read and write) on this repository |

## Contributing

`main` is protected, so all changes go through pull requests. Fork the repo, make a branch and open a PR.

## License

Same as upstream: the app is GPL-3.0 and other packages keep their own licenses (see [COPYING.md](COPYING.md)). Modrinth's branding has been removed as their license requires. The Threadrinth logo is original artwork.
