<script setup lang="ts">
import {
	CheckCircleIcon,
	CircleAlertIcon,
	ClipboardCopyIcon,
	CpuIcon,
	FolderOpenIcon,
	GlobeIcon,
	MemoryStickIcon,
	PlayIcon,
	RefreshCwIcon,
	SaveIcon,
	SendIcon,
	ServerIcon,
	SkullIcon,
	SpinnerIcon,
	StopCircleIcon,
	TrashIcon,
	UpdatedIcon,
	UsersIcon,
	XCircleIcon,
} from '@modrinth/assets'
import {
	Admonition,
	Avatar,
	Button,
	Checkbox,
	Chips,
	defineMessages,
	injectNotificationManager,
	Input,
	Slider,
	Toggle,
	useVIntl,
} from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, nextTick, onMounted, onUnmounted, ref, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import {
	check_server_reachability,
	type ConsoleLine,
	delete_server,
	edit_server,
	get_server,
	type HostedServer,
	kill_server,
	send_server_command,
	server_console,
	server_folder,
	server_properties,
	server_status,
	type ServerStatus,
	set_server_properties,
	start_server,
	stop_server,
	sync_server_mods,
} from '@/helpers/hosting'
import { get as getInstance, getInstanceIconUrl } from '@/helpers/instance'
import { openPath } from '@/helpers/utils'
import { useBreadcrumb, useRootBreadcrumb } from '@/providers/breadcrumbs'

defineOptions({ name: 'LocalServerPage' })

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()
const route = useRoute()
const router = useRouter()

const messages = defineMessages({
	host: { id: 'app.hosting.heading', defaultMessage: 'Host' },
	offline: { id: 'app.hosting.state.offline', defaultMessage: 'Offline' },
	starting: { id: 'app.hosting.state.starting', defaultMessage: 'Starting' },
	running: { id: 'app.hosting.state.running', defaultMessage: 'Online' },
	stopping: { id: 'app.hosting.state.stopping', defaultMessage: 'Stopping' },
	start: { id: 'app.hosting.start', defaultMessage: 'Start' },
	stop: { id: 'app.hosting.stop', defaultMessage: 'Stop' },
	kill: { id: 'app.hosting.kill', defaultMessage: 'Force stop' },
	console: { id: 'app.hosting.tab.console', defaultMessage: 'Console' },
	settings: { id: 'app.hosting.tab.settings', defaultMessage: 'Settings' },
	commandPlaceholder: {
		id: 'app.hosting.command-placeholder',
		defaultMessage: 'Type a command, like say Hello or op YourName',
	},
	send: { id: 'app.hosting.send', defaultMessage: 'Send' },
	cpu: { id: 'app.hosting.cpu', defaultMessage: 'CPU' },
	memory: { id: 'app.hosting.memory', defaultMessage: 'Memory' },
	players: { id: 'app.hosting.players-label', defaultMessage: 'Players' },
	address: { id: 'app.hosting.address', defaultMessage: 'Address' },
	sameNetwork: { id: 'app.hosting.address.same-network', defaultMessage: 'Same network' },
	thisComputer: { id: 'app.hosting.address.this-computer', defaultMessage: 'This computer' },
	internet: { id: 'app.hosting.address.internet', defaultMessage: 'Internet' },
	forwarded: { id: 'app.hosting.forwarding.upnp', defaultMessage: 'Port forwarded' },
	playit: { id: 'app.hosting.forwarding.playit', defaultMessage: 'playit.gg tunnel' },
	checking: {
		id: 'app.hosting.forwarding.checking',
		defaultMessage: 'Checking from the internet…',
	},
	reachable: { id: 'app.hosting.forwarding.reachable', defaultMessage: 'Works from the internet' },
	unreachable: {
		id: 'app.hosting.forwarding.unreachable',
		defaultMessage: 'Not reachable from the internet',
	},
	forwardingOff: { id: 'app.hosting.forwarding.off', defaultMessage: 'Port forwarding off' },
	forwardingFailed: {
		id: 'app.hosting.forwarding.failed',
		defaultMessage: 'Port forwarding failed',
	},
	unreachableHint: {
		id: 'app.hosting.forwarding.unreachable-hint',
		defaultMessage:
			'Your router or internet provider blocks the port. Link playit.gg on the Host page for an address that always works.',
	},
	offHint: {
		id: 'app.hosting.forwarding.off-hint',
		defaultMessage:
			'Only players on your network can join. Turn on public access in Settings to let friends join over the internet.',
	},
	checkAgain: { id: 'app.hosting.forwarding.check-again', defaultMessage: 'Check again' },
	copy: { id: 'app.hosting.copy', defaultMessage: 'Copy' },
	copied: { id: 'app.hosting.copied', defaultMessage: 'Address copied' },
	openFolder: { id: 'app.hosting.open-folder', defaultMessage: 'Open folder' },
	updateMods: { id: 'app.hosting.update-mods', defaultMessage: 'Update mods from instance' },
	modsUpdated: {
		id: 'app.hosting.mods-updated',
		defaultMessage: '{count, plural, one {# mod} other {# mods}} copied from the instance.',
	},
	delete: { id: 'app.hosting.delete', defaultMessage: 'Delete server' },
	deleteConfirm: {
		id: 'app.hosting.delete-confirm',
		defaultMessage: 'Click again to delete the server and its world for good.',
	},
	eulaNeeded: {
		id: 'app.hosting.eula-needed',
		defaultMessage: 'Accept the Minecraft EULA in Settings to start the server.',
	},
	general: { id: 'app.hosting.settings.general', defaultMessage: 'General' },
	name: { id: 'app.hosting.settings.name', defaultMessage: 'Name' },
	port: { id: 'app.hosting.settings.port', defaultMessage: 'Port' },
	publicAccess: {
		id: 'app.hosting.settings.public',
		defaultMessage: 'Public access',
	},
	publicAccessDescription: {
		id: 'app.hosting.settings.public-description',
		defaultMessage:
			'Let friends join over the internet: automatic port forwarding, or a playit.gg address (link it on the Host page).',
	},
	eula: { id: 'app.hosting.create.eula', defaultMessage: 'I accept the Minecraft EULA' },
	readEula: { id: 'app.hosting.create.read-eula', defaultMessage: 'Read the EULA' },
	game: { id: 'app.hosting.settings.game', defaultMessage: 'Game' },
	motd: { id: 'app.hosting.settings.motd', defaultMessage: 'Message of the day' },
	maxPlayers: { id: 'app.hosting.settings.max-players', defaultMessage: 'Max players' },
	gamemode: { id: 'app.hosting.settings.gamemode', defaultMessage: 'Game mode' },
	difficulty: { id: 'app.hosting.settings.difficulty', defaultMessage: 'Difficulty' },
	pvp: { id: 'app.hosting.settings.pvp', defaultMessage: 'PvP' },
	whitelist: { id: 'app.hosting.settings.whitelist', defaultMessage: 'Whitelist' },
	onlineMode: {
		id: 'app.hosting.settings.online-mode',
		defaultMessage: 'Only Minecraft accounts (online mode)',
	},
	viewDistance: { id: 'app.hosting.settings.view-distance', defaultMessage: 'View distance' },
	seed: { id: 'app.hosting.settings.seed', defaultMessage: 'World seed' },
	save: { id: 'app.hosting.settings.save', defaultMessage: 'Save' },
	saved: { id: 'app.hosting.settings.saved', defaultMessage: 'Settings saved' },
	restartNote: {
		id: 'app.hosting.settings.restart-note',
		defaultMessage: 'Changes apply the next time the server starts.',
	},
})

type Tab = 'console' | 'settings'
type InputValue = string | number | undefined
const GAMEMODES = ['survival', 'creative', 'adventure', 'spectator']
const DIFFICULTIES = ['peaceful', 'easy', 'normal', 'hard']

// Follows the route only while it is this page, so leaving it doesn't ask
// for a server named "undefined".
const id = ref(String(route.params.id))
watch(
	() => route.params.id,
	(value) => {
		if (route.name === 'HostServer' && typeof value === 'string') id.value = value
	},
)
const server = ref<HostedServer>()
const status = ref<ServerStatus>()
const lines = ref<ConsoleLine[]>([])
const command = ref<InputValue>('')
const tab = ref<Tab>('console')
const confirmDelete = ref(false)
const consoleBox = ref<HTMLElement>()
let timer: ReturnType<typeof setInterval> | undefined

// Settings form. Text fields hold what the inputs give back (numbers for
// number inputs).
const form = ref<{
	name: InputValue
	memory_mb: number
	port: InputValue
	public: boolean
	eula: boolean
	motd: InputValue
	maxPlayers: InputValue
	gamemode: string
	difficulty: string
	pvp: boolean
	whitelist: boolean
	onlineMode: boolean
	viewDistance: InputValue
	seed: InputValue
}>({
	name: '',
	memory_mb: 4096,
	port: '25565',
	public: false,
	eula: false,
	motd: '',
	maxPlayers: '20',
	gamemode: 'survival',
	difficulty: 'easy',
	pvp: true,
	whitelist: false,
	onlineMode: true,
	viewDistance: '10',
	seed: '',
})

const iconUrl = ref<string | null>(null)

// Host > server, also when opened from the running servers in the top bar.
const hostBreadcrumb = useRootBreadcrumb({
	slot: 'root',
	id: 'host',
	label: () => formatMessage(messages.host),
	to: '/host',
	visual: { type: 'icon', component: ServerIcon },
})
useBreadcrumb(
	{
		slot: 'page',
		id: () => `host-${id.value}`,
		label: () => server.value?.name ?? '',
		to: () => `/host/${encodeURIComponent(id.value)}`,
		visual: () =>
			iconUrl.value
				? { type: 'image', src: iconUrl.value, alt: server.value?.name, tintBy: id.value }
				: undefined,
	},
	{ parent: hostBreadcrumb },
)

const state = computed(() => status.value?.state ?? 'offline')
const stateLabel = computed(() => formatMessage(messages[state.value]))
const uptime = computed(() => {
	const started = status.value?.started
	if (!started) return null
	const seconds = Math.max(0, Math.floor((Date.now() - new Date(started).getTime()) / 1000))
	const h = Math.floor(seconds / 3600)
	const m = Math.floor((seconds % 3600) / 60)
	return h > 0 ? `${h}h ${m}m` : `${m}m ${seconds % 60}s`
})
const memoryUsed = computed(() => {
	const bytes = status.value?.memory_bytes
	return bytes == null ? '–' : `${(bytes / 1024 / 1024 / 1024).toFixed(1)} GB`
})
const memoryLimit = computed(() =>
	server.value ? `${(server.value.memory_mb / 1024).toFixed(1)} GB` : '',
)
const cpu = computed(() => {
	const value = status.value?.cpu_percent
	return value == null ? '–' : `${Math.round(value)}%`
})
const portSuffix = computed(() =>
	server.value && server.value.port !== 25565 ? `:${server.value.port}` : '',
)

async function load() {
	try {
		server.value = await get_server(id.value)
		const instanceId = server.value.instance_id
		iconUrl.value = instanceId
			? getInstanceIconUrl((await getInstance(instanceId).catch(() => null))?.icon_path)
			: null
		const properties = Object.fromEntries(await server_properties(id.value))
		form.value = {
			name: server.value.name,
			memory_mb: server.value.memory_mb,
			port: String(server.value.port),
			public: server.value.public_access === 'auto',
			eula: server.value.eula_accepted,
			motd: properties['motd'] ?? '',
			maxPlayers: properties['max-players'] ?? '20',
			gamemode: properties['gamemode'] ?? 'survival',
			difficulty: properties['difficulty'] ?? 'easy',
			pvp: (properties['pvp'] ?? 'true') === 'true',
			whitelist: (properties['white-list'] ?? 'false') === 'true',
			onlineMode: (properties['online-mode'] ?? 'true') === 'true',
			viewDistance: properties['view-distance'] ?? '10',
			seed: properties['level-seed'] ?? '',
		}
	} catch (error) {
		handleError(error as Error)
	}
}

async function poll() {
	try {
		status.value = await server_status(id.value)
		const last = lines.value.length > 0 ? lines.value[lines.value.length - 1].seq : null
		const added = await server_console(id.value, last)
		if (added.length > 0) {
			const box = consoleBox.value
			const atBottom = !box || box.scrollHeight - box.scrollTop - box.clientHeight < 40
			lines.value = [...lines.value, ...added].slice(-5000)
			if (atBottom) {
				await nextTick()
				consoleBox.value?.scrollTo({ top: consoleBox.value.scrollHeight })
			}
		}
	} catch (error) {
		// The server may have been deleted; stop quietly.
		console.warn(error)
	}
}

async function run(action: () => Promise<unknown>) {
	try {
		await action()
	} catch (error) {
		handleError(error as Error)
	}
	await poll()
}

async function sendCommand() {
	const text = String(command.value ?? '').trim()
	if (!text) return
	command.value = ''
	await run(() => send_server_command(id.value, text))
}

const localhostAddress = computed(() => `localhost${portSuffix.value}`)
const mainAddress = computed(
	() =>
		status.value?.public_address?.address ?? status.value?.lan_address ?? localhostAddress.value,
)
const otherAddresses = computed(() =>
	[
		{
			label: formatMessage(messages.internet),
			address: status.value?.public_address?.address,
		},
		{ label: formatMessage(messages.sameNetwork), address: status.value?.lan_address },
		{ label: formatMessage(messages.thisComputer), address: localhostAddress.value },
	].filter(
		(entry): entry is { label: string; address: string } =>
			!!entry.address && entry.address !== mainAddress.value,
	),
)

/** The port forwarding indicator next to the address. */
const forwarding = computed(() => {
	const current = status.value
	if (!current || !server.value || current.state === 'offline') return null
	if (server.value.public_access !== 'auto') {
		return {
			label: formatMessage(messages.forwardingOff),
			icon: CircleAlertIcon,
			spin: false,
			class: 'bg-button-bg text-secondary',
		}
	}
	if (!current.public_address) {
		if (current.public_error) {
			return {
				label: formatMessage(messages.forwardingFailed),
				icon: XCircleIcon,
				spin: false,
				class: 'bg-highlight-red text-red',
			}
		}
		return {
			label: formatMessage(messages.checking),
			icon: SpinnerIcon,
			spin: true,
			class: 'bg-button-bg text-secondary',
		}
	}
	const via = formatMessage(
		current.public_address.via === 'upnp' ? messages.forwarded : messages.playit,
	)
	switch (current.reachability) {
		case 'reachable':
			return {
				label: `${via} · ${formatMessage(messages.reachable)}`,
				icon: CheckCircleIcon,
				spin: false,
				class: 'bg-highlight-green text-green',
			}
		case 'unreachable':
			return {
				label: `${via} · ${formatMessage(messages.unreachable)}`,
				icon: XCircleIcon,
				spin: false,
				class: 'bg-highlight-red text-red',
			}
		case 'checking':
			return {
				label: `${via} · ${formatMessage(messages.checking)}`,
				icon: SpinnerIcon,
				spin: true,
				class: 'bg-button-bg text-secondary',
			}
		default:
			return {
				label: via,
				icon: CheckCircleIcon,
				spin: false,
				class: 'bg-button-bg text-primary',
			}
	}
})
const forwardingHint = computed(() => {
	if (!status.value || status.value.state === 'offline') return null
	if (server.value?.public_access !== 'auto') return formatMessage(messages.offHint)
	if (status.value.reachability === 'unreachable') return formatMessage(messages.unreachableHint)
	return null
})

async function checkReachability() {
	try {
		await check_server_reachability(id.value)
	} catch (error) {
		handleError(error as Error)
	}
}

async function copyAddress(address: string) {
	await navigator.clipboard.writeText(address)
	addNotification({ title: formatMessage(messages.copied), text: address, type: 'success' })
}

async function saveSettings() {
	try {
		const text = (value: InputValue) => String(value ?? '').trim()
		const port = Number.parseInt(text(form.value.port), 10)
		server.value = await edit_server(id.value, {
			name: text(form.value.name),
			memory_mb: form.value.memory_mb,
			port: Number.isFinite(port) ? port : undefined,
			public_access: form.value.public ? 'auto' : 'off',
			eula_accepted: form.value.eula,
		})
		await set_server_properties(id.value, {
			motd: text(form.value.motd),
			'max-players': text(form.value.maxPlayers),
			gamemode: form.value.gamemode,
			difficulty: form.value.difficulty,
			pvp: String(form.value.pvp),
			'white-list': String(form.value.whitelist),
			'enforce-whitelist': String(form.value.whitelist),
			'online-mode': String(form.value.onlineMode),
			'view-distance': text(form.value.viewDistance),
			'level-seed': text(form.value.seed),
		})
		addNotification({ title: formatMessage(messages.saved), type: 'success' })
		await load()
	} catch (error) {
		handleError(error as Error)
	}
}

async function updateMods() {
	try {
		const count = await sync_server_mods(id.value)
		addNotification({
			title: formatMessage(messages.updateMods),
			text: formatMessage(messages.modsUpdated, { count }),
			type: 'success',
		})
	} catch (error) {
		handleError(error as Error)
	}
}

async function removeServer() {
	if (!confirmDelete.value) {
		confirmDelete.value = true
		return
	}
	try {
		await delete_server(id.value)
		await router.push('/host')
	} catch (error) {
		handleError(error as Error)
	}
}

async function openFolder() {
	await run(async () => openPath(await server_folder(id.value)))
}

// Coming back to the console shows the newest lines.
watch(tab, async (value) => {
	if (value !== 'console') return
	await nextTick()
	consoleBox.value?.scrollTo({ top: consoleBox.value.scrollHeight })
})

watch(id, () => {
	lines.value = []
	confirmDelete.value = false
	void load()
	void poll()
})

onMounted(() => {
	void load()
	void poll()
	timer = setInterval(() => void poll(), 1000)
})
onUnmounted(() => {
	if (timer) clearInterval(timer)
})
</script>

<template>
	<div v-if="server" class="flex h-full flex-col gap-4 p-6">
		<div class="flex flex-wrap items-center justify-between gap-4">
			<div class="flex items-center gap-3">
				<Avatar :src="iconUrl" :alt="server.name" :tint-by="id" size="64px" />
				<div class="flex flex-col gap-1">
					<h1 class="m-0 text-2xl font-extrabold text-contrast">{{ server.name }}</h1>
					<span class="text-sm text-secondary">
						{{ server.loader }} {{ server.game_version }} ·
						<span
							:class="{
								'text-green': state === 'running',
								'text-orange': state === 'starting' || state === 'stopping',
							}"
							>{{ stateLabel }}</span
						>
						<template v-if="uptime"> · {{ uptime }}</template>
					</span>
				</div>
			</div>
			<div class="flex flex-wrap items-center gap-2">
				<Button
					v-if="state === 'offline'"
					type="colored"
					color="brand"
					:disabled="!server.eula_accepted"
					@click="run(() => start_server(id))"
				>
					<PlayIcon />
					{{ formatMessage(messages.start) }}
				</Button>
				<template v-else>
					<Button type="outlined" @click="run(() => stop_server(id))">
						<StopCircleIcon />
						{{ formatMessage(messages.stop) }}
					</Button>
					<Button type="outlined" color="red" @click="run(() => kill_server(id))">
						<SkullIcon />
						{{ formatMessage(messages.kill) }}
					</Button>
				</template>
				<Button type="outlined" @click="openFolder">
					<FolderOpenIcon />
					{{ formatMessage(messages.openFolder) }}
				</Button>
			</div>
		</div>

		<Admonition
			v-if="!server.eula_accepted"
			type="warning"
			:header="formatMessage(messages.eulaNeeded)"
		/>

		<div class="grid grid-cols-3 gap-3">
			<div class="flex flex-col gap-1 rounded-2xl bg-bg-raised p-4">
				<span class="flex items-center gap-2 text-sm text-secondary">
					<CpuIcon /> {{ formatMessage(messages.cpu) }}
				</span>
				<span class="text-xl font-bold text-contrast">{{ cpu }}</span>
			</div>
			<div class="flex flex-col gap-1 rounded-2xl bg-bg-raised p-4">
				<span class="flex items-center gap-2 text-sm text-secondary">
					<MemoryStickIcon /> {{ formatMessage(messages.memory) }}
				</span>
				<span class="text-xl font-bold text-contrast">
					{{ memoryUsed }} <span class="text-sm text-secondary">/ {{ memoryLimit }}</span>
				</span>
			</div>
			<div class="flex flex-col gap-1 rounded-2xl bg-bg-raised p-4">
				<span class="flex items-center gap-2 text-sm text-secondary">
					<UsersIcon /> {{ formatMessage(messages.players) }}
				</span>
				<span class="text-xl font-bold text-contrast">{{ status?.players.length ?? 0 }}</span>
				<span v-if="status?.players.length" class="truncate text-sm text-secondary">
					{{ status.players.join(', ') }}
				</span>
			</div>
		</div>

		<div class="flex flex-col gap-3 rounded-2xl bg-bg-raised p-4">
			<div class="flex flex-wrap items-center gap-3">
				<span class="flex items-center gap-2 text-sm text-secondary">
					<GlobeIcon /> {{ formatMessage(messages.address) }}
				</span>
				<span
					v-if="forwarding"
					class="flex items-center gap-1 rounded-full px-2 py-0.5 text-sm font-semibold"
					:class="forwarding.class"
				>
					<component :is="forwarding.icon" :class="{ 'animate-spin': forwarding.spin }" />
					{{ forwarding.label }}
				</span>
				<Button
					v-if="status?.public_address && status.reachability !== 'checking'"
					type="quiet"
					size="sm"
					@click="checkReachability"
				>
					<RefreshCwIcon />
					{{ formatMessage(messages.checkAgain) }}
				</Button>
			</div>
			<div class="flex flex-wrap items-center gap-2">
				<span class="truncate text-2xl font-bold text-contrast">{{ mainAddress }}</span>
				<Button type="outlined" size="sm" @click="copyAddress(mainAddress)">
					<ClipboardCopyIcon />
					{{ formatMessage(messages.copy) }}
				</Button>
			</div>
			<p v-if="forwardingHint" class="m-0 text-sm text-secondary">{{ forwardingHint }}</p>
			<p v-if="status?.public_error" class="m-0 text-sm text-orange">{{ status.public_error }}</p>
			<div class="flex flex-wrap gap-2">
				<button
					v-for="entry in otherAddresses"
					:key="entry.label"
					v-tooltip="formatMessage(messages.copy)"
					class="flex items-center gap-2 rounded-xl border-none bg-button-bg px-3 py-2 text-sm text-primary hover:brightness-110"
					@click="copyAddress(entry.address)"
				>
					<span class="text-secondary">{{ entry.label }}</span>
					<span class="font-semibold text-contrast">{{ entry.address }}</span>
					<ClipboardCopyIcon />
				</button>
			</div>
		</div>

		<Chips
			v-model="tab"
			:items="['console', 'settings'] as Tab[]"
			:format-label="(item: Tab) => formatMessage(messages[item])"
		/>

		<div v-if="tab === 'console'" class="flex min-h-[20rem] flex-1 flex-col gap-2">
			<div
				ref="consoleBox"
				class="min-h-[20rem] flex-1 overflow-auto rounded-2xl bg-bg-raised p-4 font-mono text-sm"
			>
				<div
					v-for="line in lines"
					:key="line.seq"
					class="whitespace-pre-wrap break-all"
					:class="{
						'text-red': line.stream === 'error',
						'text-brand': line.stream === 'input',
						'text-secondary italic': line.stream === 'app',
					}"
				>
					{{ line.text }}
				</div>
			</div>
			<form class="flex gap-2" @submit.prevent="sendCommand">
				<Input
					v-model="command"
					type="text"
					:placeholder="formatMessage(messages.commandPlaceholder)"
					:disabled="state === 'offline'"
					wrapper-class="flex-1"
				/>
				<Button type="colored" color="brand" native-type="submit" :disabled="state === 'offline'">
					<SendIcon />
					{{ formatMessage(messages.send) }}
				</Button>
			</form>
		</div>

		<div v-else class="flex flex-col gap-6">
			<Admonition type="info" :header="formatMessage(messages.restartNote)" />
			<section class="flex flex-col gap-3">
				<h2 class="m-0 text-lg font-bold text-contrast">{{ formatMessage(messages.general) }}</h2>
				<label class="flex flex-col gap-2">
					<span class="font-semibold">{{ formatMessage(messages.name) }}</span>
					<Input v-model="form.name" type="text" wrapper-class="w-full max-w-md" />
				</label>
				<div class="flex flex-col gap-2">
					<span class="font-semibold">{{ formatMessage(messages.memory) }}</span>
					<Slider v-model="form.memory_mb" :min="1024" :max="16384" :step="512" unit="MB" />
				</div>
				<label class="flex flex-col gap-2">
					<span class="font-semibold">{{ formatMessage(messages.port) }}</span>
					<Input v-model="form.port" type="number" wrapper-class="w-40" />
				</label>
				<div class="flex items-center justify-between gap-4">
					<div class="flex flex-col">
						<span class="font-semibold">{{ formatMessage(messages.publicAccess) }}</span>
						<span class="text-sm text-secondary">
							{{ formatMessage(messages.publicAccessDescription) }}
						</span>
					</div>
					<Toggle id="hosting-public-access" v-model="form.public" />
				</div>
				<div class="flex flex-wrap items-center gap-3">
					<Checkbox v-model="form.eula" :label="formatMessage(messages.eula)" />
					<button
						class="border-none bg-transparent p-0 text-link underline"
						@click="openUrl('https://aka.ms/MinecraftEULA')"
					>
						{{ formatMessage(messages.readEula) }}
					</button>
				</div>
			</section>
			<section class="flex flex-col gap-3">
				<h2 class="m-0 text-lg font-bold text-contrast">{{ formatMessage(messages.game) }}</h2>
				<label class="flex flex-col gap-2">
					<span class="font-semibold">{{ formatMessage(messages.motd) }}</span>
					<Input v-model="form.motd" type="text" wrapper-class="w-full max-w-md" />
				</label>
				<div class="flex flex-col gap-2">
					<span class="font-semibold">{{ formatMessage(messages.gamemode) }}</span>
					<Chips v-model="form.gamemode" :items="GAMEMODES" capitalize />
				</div>
				<div class="flex flex-col gap-2">
					<span class="font-semibold">{{ formatMessage(messages.difficulty) }}</span>
					<Chips v-model="form.difficulty" :items="DIFFICULTIES" capitalize />
				</div>
				<div class="flex flex-wrap gap-6">
					<label class="flex flex-col gap-2">
						<span class="font-semibold">{{ formatMessage(messages.maxPlayers) }}</span>
						<Input v-model="form.maxPlayers" type="number" wrapper-class="w-32" />
					</label>
					<label class="flex flex-col gap-2">
						<span class="font-semibold">{{ formatMessage(messages.viewDistance) }}</span>
						<Input v-model="form.viewDistance" type="number" wrapper-class="w-32" />
					</label>
				</div>
				<label class="flex flex-col gap-2">
					<span class="font-semibold">{{ formatMessage(messages.seed) }}</span>
					<Input v-model="form.seed" type="text" wrapper-class="w-full max-w-md" />
				</label>
				<div class="flex flex-col gap-3">
					<Checkbox v-model="form.pvp" :label="formatMessage(messages.pvp)" />
					<Checkbox v-model="form.whitelist" :label="formatMessage(messages.whitelist)" />
					<Checkbox v-model="form.onlineMode" :label="formatMessage(messages.onlineMode)" />
				</div>
			</section>
			<div class="flex flex-wrap items-center gap-2">
				<Button type="colored" color="brand" @click="saveSettings">
					<SaveIcon />
					{{ formatMessage(messages.save) }}
				</Button>
				<Button
					v-if="server.instance_id"
					type="outlined"
					:disabled="state !== 'offline'"
					@click="updateMods"
				>
					<UpdatedIcon />
					{{ formatMessage(messages.updateMods) }}
				</Button>
				<Button type="outlined" color="red" :disabled="state !== 'offline'" @click="removeServer">
					<TrashIcon />
					{{ formatMessage(messages.delete) }}
				</Button>
				<span v-if="confirmDelete" class="text-red">{{
					formatMessage(messages.deleteConfirm)
				}}</span>
			</div>
		</div>
	</div>
</template>
