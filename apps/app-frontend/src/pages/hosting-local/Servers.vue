<script setup lang="ts">
import { GlobeIcon, PlayIcon, PlusIcon, ServerIcon, StopCircleIcon, XIcon } from '@modrinth/assets'
import {
	Avatar,
	Button,
	defineMessages,
	EmptyState,
	injectNotificationManager,
	useVIntl,
} from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, onActivated, onMounted, onUnmounted, ref } from 'vue'
import { useRouter } from 'vue-router'

import HostServerModal from '@/components/ui/hosting/HostServerModal.vue'
import {
	type HostedServer,
	list_servers,
	playit_link_status,
	playit_start_link,
	playit_unlink,
	type PlayitLink,
	server_status,
	type ServerStatus,
	start_server,
	stop_server,
} from '@/helpers/hosting'
import { getInstanceIconUrl, list as listInstances } from '@/helpers/instance'
import { useRootBreadcrumb } from '@/providers/breadcrumbs'

defineOptions({ name: 'LocalServersPage' })

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const router = useRouter()

const messages = defineMessages({
	heading: { id: 'app.hosting.heading', defaultMessage: 'Host' },
	description: {
		id: 'app.hosting.description',
		defaultMessage:
			'Run any instance as a server on this computer, with only the mods a server needs.',
	},
	hostInstance: { id: 'app.hosting.host-instance', defaultMessage: 'Host an instance' },
	emptyHeading: { id: 'app.hosting.empty.heading', defaultMessage: 'No servers yet' },
	emptyDescription: {
		id: 'app.hosting.empty.description',
		defaultMessage:
			'Pick an instance to host. Its Minecraft version, mod loader and server-side mods are set up for you.',
	},
	start: { id: 'app.hosting.start', defaultMessage: 'Start' },
	stop: { id: 'app.hosting.stop', defaultMessage: 'Stop' },
	offline: { id: 'app.hosting.state.offline', defaultMessage: 'Offline' },
	starting: { id: 'app.hosting.state.starting', defaultMessage: 'Starting' },
	running: { id: 'app.hosting.state.running', defaultMessage: 'Online' },
	stopping: { id: 'app.hosting.state.stopping', defaultMessage: 'Stopping' },
	players: {
		id: 'app.hosting.players',
		defaultMessage: '{count, plural, one {# player} other {# players}}',
	},
	playitHeading: { id: 'app.hosting.playit.heading', defaultMessage: 'Play with friends online' },
	playitDescription: {
		id: 'app.hosting.playit.description',
		defaultMessage:
			'Servers with public access first try your router’s automatic port forwarding. Where that doesn’t work, a free playit.gg account gives them a public address.',
	},
	playitLinked: {
		id: 'app.hosting.playit.linked',
		defaultMessage: 'playit.gg is linked.',
	},
	playitLinking: {
		id: 'app.hosting.playit.linking',
		defaultMessage: 'Confirm in your browser to finish linking.',
	},
	playitLink: { id: 'app.hosting.playit.link', defaultMessage: 'Link playit.gg' },
	playitReopen: { id: 'app.hosting.playit.reopen', defaultMessage: 'Open the page again' },
	playitUnlink: { id: 'app.hosting.playit.unlink', defaultMessage: 'Unlink' },
})

const breadcrumb = useRootBreadcrumb({
	slot: 'root',
	id: 'host',
	label: formatMessage(messages.heading),
	to: '/host',
	visual: { type: 'icon', component: ServerIcon },
})
onActivated(breadcrumb.reset)

const servers = ref<HostedServer[]>([])
const instanceIcons = ref(new Map<string, string | null>())
const statuses = ref<Record<string, ServerStatus>>({})
const playit = ref<PlayitLink>({ linked: false, link_url: null, error: null })
const hostModal = ref<InstanceType<typeof HostServerModal>>()
let timer: ReturnType<typeof setInterval> | undefined

const stateLabels = computed(() => ({
	offline: formatMessage(messages.offline),
	starting: formatMessage(messages.starting),
	running: formatMessage(messages.running),
	stopping: formatMessage(messages.stopping),
}))

async function refresh() {
	try {
		servers.value = await list_servers()
		if (
			servers.value.some(
				(server) => server.instance_id && !instanceIcons.value.has(server.instance_id),
			)
		) {
			const instances = await listInstances()
			instanceIcons.value = new Map(
				instances.map((instance) => [instance.id, getInstanceIconUrl(instance.icon_path)]),
			)
		}
		const entries = await Promise.all(
			servers.value.map(async (server) => [server.id, await server_status(server.id)] as const),
		)
		statuses.value = Object.fromEntries(entries)
		playit.value = await playit_link_status()
	} catch (error) {
		handleError(error as Error)
	}
}

async function toggle(server: HostedServer) {
	try {
		if ((statuses.value[server.id]?.state ?? 'offline') === 'offline') {
			await start_server(server.id)
		} else {
			await stop_server(server.id)
		}
	} catch (error) {
		handleError(error as Error)
	}
	await refresh()
}

async function linkPlayit() {
	try {
		const url = await playit_start_link()
		await openUrl(url)
		await refresh()
	} catch (error) {
		handleError(error as Error)
	}
}

async function unlinkPlayit() {
	try {
		await playit_unlink()
		await refresh()
	} catch (error) {
		handleError(error as Error)
	}
}

onMounted(() => {
	void refresh()
	timer = setInterval(() => void refresh(), 2000)
})
onUnmounted(() => {
	if (timer) clearInterval(timer)
})
</script>

<template>
	<div class="flex flex-col gap-4 p-6">
		<HostServerModal ref="hostModal" @created="() => void refresh()" />
		<div class="flex flex-wrap items-center justify-between gap-4">
			<div class="flex flex-col gap-1">
				<h1 class="m-0 text-2xl font-extrabold text-contrast">
					{{ formatMessage(messages.heading) }}
				</h1>
				<p class="m-0 text-secondary">{{ formatMessage(messages.description) }}</p>
			</div>
			<Button type="colored" color="brand" @click="hostModal?.show()">
				<PlusIcon />
				{{ formatMessage(messages.hostInstance) }}
			</Button>
		</div>

		<div class="flex flex-col gap-3 rounded-2xl bg-bg-raised p-4">
			<div class="flex items-center gap-2 font-semibold text-contrast">
				<GlobeIcon />
				{{ formatMessage(messages.playitHeading) }}
			</div>
			<p class="m-0 text-sm text-secondary">{{ formatMessage(messages.playitDescription) }}</p>
			<div class="flex flex-wrap items-center gap-3">
				<template v-if="playit.linked">
					<span class="text-green">{{ formatMessage(messages.playitLinked) }}</span>
					<Button type="outlined" @click="unlinkPlayit">
						<XIcon />
						{{ formatMessage(messages.playitUnlink) }}
					</Button>
				</template>
				<template v-else-if="playit.link_url">
					<span>{{ formatMessage(messages.playitLinking) }}</span>
					<Button type="outlined" @click="openUrl(playit.link_url ?? '')">
						{{ formatMessage(messages.playitReopen) }}
					</Button>
				</template>
				<Button v-else type="outlined" @click="linkPlayit">
					<GlobeIcon />
					{{ formatMessage(messages.playitLink) }}
				</Button>
				<span v-if="playit.error" class="text-red">{{ playit.error }}</span>
			</div>
		</div>

		<EmptyState
			v-if="servers.length === 0"
			:heading="formatMessage(messages.emptyHeading)"
			:description="formatMessage(messages.emptyDescription)"
		/>
		<div v-else class="flex flex-col gap-2">
			<div
				v-for="server in servers"
				:key="server.id"
				class="flex cursor-pointer items-center gap-4 rounded-2xl bg-bg-raised p-4 hover:brightness-110"
				@click="router.push(`/host/${encodeURIComponent(server.id)}`)"
			>
				<Avatar
					:src="server.instance_id ? instanceIcons.get(server.instance_id) : null"
					:alt="server.name"
					:tint-by="server.id"
					size="48px"
				/>
				<div class="flex min-w-0 flex-1 flex-col gap-1">
					<span class="truncate font-semibold text-contrast">{{ server.name }}</span>
					<span class="text-sm text-secondary">
						{{ server.loader }} {{ server.game_version }} ·
						{{ stateLabels[statuses[server.id]?.state ?? 'offline'] }}
						<template v-if="statuses[server.id]?.state === 'running'">
							·
							{{
								formatMessage(messages.players, {
									count: statuses[server.id]?.players.length ?? 0,
								})
							}}
						</template>
						<template v-if="statuses[server.id]?.public_address">
							· {{ statuses[server.id]?.public_address?.address }}
						</template>
					</span>
				</div>
				<Button
					v-if="(statuses[server.id]?.state ?? 'offline') === 'offline'"
					type="colored"
					color="brand"
					@click.stop="toggle(server)"
				>
					<PlayIcon />
					{{ formatMessage(messages.start) }}
				</Button>
				<Button v-else type="outlined" @click.stop="toggle(server)">
					<StopCircleIcon />
					{{ formatMessage(messages.stop) }}
				</Button>
			</div>
		</div>
	</div>
</template>
