import { invoke } from '@tauri-apps/api/core'

/** Commands for the CurseForge tab (CurseForge mods and modpacks, Feed the Beast modpacks). */

export type CurseForgeClass = 'mod' | 'modpack'
export type CurseForgeSort = 'popularity' | 'updated' | 'downloads' | 'name'
export type CurseForgeLoader = 'forge' | 'neoforge' | 'fabric' | 'quilt'

export type CurseForgeSearchQuery = {
	class: CurseForgeClass
	query?: string | null
	game_version?: string | null
	loader?: CurseForgeLoader | null
	sort: CurseForgeSort
	index?: number
	page_size?: number
}

export type CurseForgeProject = {
	id: number
	name: string
	slug: string
	summary: string
	authors: string[]
	downloads: number
	icon_url: string | null
	website_url: string | null
	updated: string | null
	allow_distribution: boolean
	game_versions: string[]
	loaders: string[]
}

export type CurseForgeSearchResults = {
	projects: CurseForgeProject[]
	index: number
	page_size: number
	total: number
}

export type CurseForgeFile = {
	id: number
	project_id: number
	name: string
	file_name: string
	release_type: 'release' | 'beta' | 'alpha'
	date: string
	size: number
	game_versions: string[]
	loaders: string[]
	downloadable: boolean
	page_url: string
}

export type CurseForgeFiles = {
	files: CurseForgeFile[]
	index: number
	total: number
}

export type ManualDownload = {
	name: string
	url: string
	folder: string
}

export type CurseForgeModInstall = {
	installed: string[]
	manual_downloads: ManualDownload[]
	missing_dependencies: string[]
}

export type PackInstallReport = {
	job: { job_id: string; instance_id: string | null }
	instance_name: string
	manual_downloads: ManualDownload[]
}

export type FtbVersion = {
	id: number
	name: string
	release_type: string
	updated: number
	game_version: string | null
	loader: string | null
	loader_version: string | null
}

export type FtbPack = {
	id: number
	name: string
	summary: string
	icon_url: string | null
	authors: string[]
	installs: number
	plays: number
	updated: number
	tags: string[]
	website_url: string
	versions: FtbVersion[]
}

export type FtbSearchQuery = {
	query?: string | null
	game_version?: string | null
	loader?: string | null
	limit?: number
}

/** Whether this build has a CurseForge API key. Feed the Beast works without one. */
export async function curseforge_status(): Promise<{ available: boolean }> {
	return await invoke('plugin:curseforge|curseforge_status')
}

export async function curseforge_search(
	query: CurseForgeSearchQuery,
): Promise<CurseForgeSearchResults> {
	return await invoke('plugin:curseforge|curseforge_search', { query })
}

export async function curseforge_files(
	projectId: number,
	gameVersion?: string | null,
	loader?: string | null,
	index?: number,
): Promise<CurseForgeFiles> {
	return await invoke('plugin:curseforge|curseforge_files', {
		projectId,
		gameVersion: gameVersion || null,
		loader: loader || null,
		index: index ?? null,
	})
}

/** Installs a mod (the given file, or the newest that fits) and its required dependencies. */
export async function curseforge_install_mod(
	instanceId: string,
	projectId: number,
	fileId?: number | null,
): Promise<CurseForgeModInstall> {
	return await invoke('plugin:curseforge|curseforge_install_mod', {
		instanceId,
		projectId,
		fileId: fileId ?? null,
	})
}

/** Starts installing a CurseForge modpack as a new instance. */
export async function curseforge_install_modpack(
	projectId: number,
	fileId?: number | null,
): Promise<PackInstallReport> {
	return await invoke('plugin:curseforge|curseforge_install_modpack', {
		projectId,
		fileId: fileId ?? null,
	})
}

export async function ftb_search(query: FtbSearchQuery): Promise<FtbPack[]> {
	return await invoke('plugin:curseforge|ftb_search', { query })
}

/** Starts installing a Feed the Beast pack version as a new instance. */
export async function ftb_install_pack(
	packId: number,
	versionId: number,
): Promise<PackInstallReport> {
	return await invoke('plugin:curseforge|ftb_install_pack', { packId, versionId })
}
