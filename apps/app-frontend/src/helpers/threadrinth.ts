import { invoke } from '@tauri-apps/api/core'

/** Threadrinth release locations on GitHub. */
export const THREADRINTH_REPOSITORY_URL = 'https://github.com/Georgwav/Threadrinth'
export const THREADRINTH_RELEASES_URL = `${THREADRINTH_REPOSITORY_URL}/releases`
/** Updater manifest published with every release (same file the in-app updater reads). */
export const THREADRINTH_UPDATE_MANIFEST_URL = `${THREADRINTH_RELEASES_URL}/latest/download/latest.json`

export type WorldTransferMode = 'copy' | 'move'

/** Copies or moves a singleplayer world to another instance. Returns its folder name there. */
export async function transfer_world(
	fromInstance: string,
	world: string,
	toInstance: string,
	mode: WorldTransferMode,
): Promise<string> {
	return await invoke('plugin:threadrinth|transfer_world', {
		fromInstance,
		world,
		toInstance,
		mode,
	})
}

export type ServerPackReport = {
	mods_included: number
	client_only_mods: string[]
	unknown_mods: string[]
}

/** Exports an instance as a ready-to-run server zip. */
export async function export_server_pack(
	instanceId: string,
	exportLocation: string,
): Promise<ServerPackReport> {
	return await invoke('plugin:threadrinth|export_server_pack', { instanceId, exportLocation })
}
