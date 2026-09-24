# ![Threadrinth](branding/threadrinth-wordmark.svg)

**Threadrinth** is a Minecraft launcher forked from the [Modrinth App](https://github.com/modrinth/code). It's the same launcher, with a few changes on top, and it stays up to date with upstream Modrinth.

> Not affiliated with or endorsed by Modrinth / Rinth, Inc.

## What's different

- **Folder-based instances, like Prism Launcher.** Each instance folder has an `instance.cfg`. On startup, or when you click **Refresh** in the library, the launcher scans the instances folder. Any folder it doesn't know yet is imported, and renamed folders are picked up. Nothing is ever deleted.
- **Works across Windows and Linux.** A database written by a Windows build opens fine on Linux and the other way around, because migrations that differ only in line endings are accepted. If a database is truly incompatible, it's kept as a backup and your instances come back from their `instance.cfg` files.
- **Separate from the official app.** Threadrinth has its own data folder and doesn't use Modrinth's auto-updater.

See [ROADMAP.md](ROADMAP.md) for what's next.

## Where things live

| Path | What |
| --- | --- |
| `apps/app` | Desktop shell (Tauri) |
| `apps/app-frontend` | Launcher UI (Vue) |
| `packages/app-lib` | Launcher core (instances, downloads, launching) |
| `branding/` | Logo sources. Run `branding/generate-assets.sh` to rebuild the icons |

Other folders (website, API, and so on) come from upstream and are left as they are.

## Development

```sh
pnpm install
pnpm app:dev
```

Requires Node.js, pnpm, Rust and the [Tauri prerequisites](https://v2.tauri.app/start/prerequisites/). Before building, copy `packages/app-lib/.env.prod` to `packages/app-lib/.env`.

## Staying on the newest Modrinth version

This fork keeps Modrinth's full git history, so their updates merge on top of our changes:

```sh
git remote add upstream https://github.com/modrinth/code.git   # once
git fetch upstream
git merge upstream/main
```

Our changes stay small and separate from upstream code to keep merges easy.

## Contributing

`main` is protected, so all changes go through pull requests. Fork the repo, make a branch, and open a PR.

## License

Same as upstream: the app is GPL-3.0 and other packages keep their own licenses (see [COPYING.md](COPYING.md)). Modrinth's branding has been removed as their license requires. The Threadrinth logo is original artwork.
