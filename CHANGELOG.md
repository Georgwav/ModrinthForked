# Changelog

Each release's GitHub description is taken from its section here (see `scripts/release-notes.py`).

## 0.21.8

- The CurseForge tab looks and works like Discover, with tabs for Modpacks, FTB Modpacks, Mods, Resource Packs, Data Packs and Shaders, filters, sorting and pages.
- Click a CurseForge or Feed the Beast project to see its page, with the description and all versions.
- Install CurseForge resource packs, data packs and shaders into an instance.
- Feed the Beast modpacks load again (they use FTB's current address now).
- Fixed linking playit.gg ("AgentVersionTooOld").
- A server's page shows its addresses with copy buttons, and whether port forwarding works from the internet.
- Running servers show in the top bar, like running instances; click to go back to them.
- Instance and world icons in the Host dialog, and the instance's icon on its server.

## 0.21.7

- Host any instance as a server: click Host as server in its menu. The Minecraft version, mod loader and server-side mods are set up for you (client-only mods are left out), with a new or copied world.
- The Host page shows each server's console (type commands), players, CPU and memory, and has settings for the port, memory, game mode, difficulty, whitelist and more.
- Friends can join over the internet: automatic port forwarding on your router, or a free playit.gg address, linked by confirming in your browser.
- A CurseForge tab below Discover: install CurseForge mods into an instance and CurseForge or Feed the Beast modpacks as new instances.
- Instance icons in the copy and move world picker.
- Modpack instances without an icon get the modpack's icon.

## 0.21.6

- Move a world to another instance straight from its menu.
- Worlds show up again on drives where their lock file can't be opened.
- Server pack export lets you pick the files, like the modpack export, and works for instances without a saved mod loader version.
- Modpack instances keep their icons, also across Windows and Linux.
- Fixed "invalid utf-8 sequence" database errors after moving the app folder.
- Fixed "Update all" failing with "The updated filename belongs to another content item".
- No more "CancelledError" popups.

## 0.21.5

- Copy or move worlds between instances from the Worlds tab.
- Export an instance as a server pack (in the instance's ⋯ menu, under Export).
- Instance icons come from `icon.png` in the instance folder, so they show up on every install.
- With a custom app folder, settings, accounts and skins are kept in that folder too, so installs sharing it (like a dual boot) share them.

## 0.21.4

- Modrinth's anonymous usage statistics are now off by default.
- Prepared for Windows code signing and winget.

## 0.21.3

- Skin history: every skin you wear stays on the skins page, even if you change it on minecraft.net.

## 0.21.2

- New themes: Ember, Sand, Orchid, Amethyst and Blossom.
- White accent color.
- Threadrinth logo on the startup screen.
- Imported instances keep their icons.

## 0.21.1

- Imports your Modrinth App instances with their version, mod loader and playtime.
- Imported instances are ready to play without a repair.
- Refresh instances from the welcome screen.
- Buttons follow your accent color.
- Fixes for copied instance folders.

## 0.21.0

- First Threadrinth release.
- Drop an instance folder into the instances folder and click Refresh, like in Prism Launcher.
- Works across Windows and Linux with one shared instances folder.
- Pick any accent color.
- Updates itself in one click.
