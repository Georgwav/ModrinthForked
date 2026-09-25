/**
 * Hosting servers built from instances on this computer (Threadrinth).
 */
import { invoke } from '@tauri-apps/api/core'

export type LaunchTarget = { type: 'jar'; path: string } | { type: 'args_file'; path: string }

export type PublicAccess = 'off' | 'auto'

export type HostedServer = {
	id: string
	name: string
	instance_id: string | null
	game_version: string
	loader: string
	loader_version: string | null
	memory_mb: number
	port: number
	eula_accepted: boolean
	launch: LaunchTarget
	selection: { included: string[]; excluded: string[] } | null
	public_access: PublicAccess
	created: string
}

export type WorldSource =
	| { type: 'new'; seed: string | null }
	| { type: 'copy'; instance_id: string; world: string }

export type CreateServer = {
	instance_id: string
	name: string
	included: string[]
	excluded: string[]
	world: WorldSource
	memory_mb: number | null
	eula_accepted: boolean
}

export type EditServer = {
	name?: string
	memory_mb?: number
	port?: number
	eula_accepted?: boolean
	public_access?: PublicAccess
}

export type ServerState = 'offline' | 'starting' | 'running' | 'stopping'

export type PublicAddress = { address: string; via: 'upnp' | 'playit' }

export type ServerStatus = {
	state: ServerState
	started: string | null
	players: string[]
	cpu_percent: number | null
	memory_bytes: number | null
	exit_code: number | null
	public_address: PublicAddress | null
	public_error: string | null
}

export type ConsoleLine = {
	seq: number
	stream: 'output' | 'error' | 'input' | 'app'
	text: string
}

export type PlayitLink = {
	linked: boolean
	link_url: string | null
	error: string | null
}

export async function list_servers(): Promise<HostedServer[]> {
	return await invoke('plugin:hosting|hosting_list')
}

export async function get_server(id: string): Promise<HostedServer> {
	return await invoke('plugin:hosting|hosting_get', { id })
}

export async function create_server(request: CreateServer): Promise<HostedServer> {
	return await invoke('plugin:hosting|hosting_create', { request })
}

export async function edit_server(id: string, edit: EditServer): Promise<HostedServer> {
	return await invoke('plugin:hosting|hosting_edit', { id, edit })
}

export async function delete_server(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_delete', { id })
}

export async function start_server(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_start', { id })
}

export async function stop_server(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_stop', { id })
}

export async function kill_server(id: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_kill', { id })
}

export async function send_server_command(id: string, command: string): Promise<void> {
	return await invoke('plugin:hosting|hosting_command', { id, command })
}

export async function server_console(id: string, after: number | null): Promise<ConsoleLine[]> {
	return await invoke('plugin:hosting|hosting_console', { id, after })
}

export async function server_status(id: string): Promise<ServerStatus> {
	return await invoke('plugin:hosting|hosting_status', { id })
}

export async function server_properties(id: string): Promise<[string, string][]> {
	return await invoke('plugin:hosting|hosting_properties', { id })
}

export async function set_server_properties(
	id: string,
	values: Record<string, string>,
): Promise<void> {
	return await invoke('plugin:hosting|hosting_set_properties', { id, values })
}

export async function sync_server_mods(id: string): Promise<number> {
	return await invoke('plugin:hosting|hosting_sync_mods', { id })
}

export async function server_folder(id: string): Promise<string> {
	return await invoke('plugin:hosting|hosting_folder', { id })
}

export async function playit_link_status(): Promise<PlayitLink> {
	return await invoke('plugin:hosting|playit_link_status')
}

export async function playit_start_link(): Promise<string> {
	return await invoke('plugin:hosting|playit_start_link')
}

export async function playit_unlink(): Promise<void> {
	return await invoke('plugin:hosting|playit_unlink')
}
