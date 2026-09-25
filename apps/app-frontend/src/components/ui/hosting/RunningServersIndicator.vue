<script setup lang="ts">
import { ServerIcon } from '@modrinth/assets'
import { defineMessages, useVIntl } from '@modrinth/ui'
import { computed, onMounted, onUnmounted, ref } from 'vue'

import { type HostedServer, list_servers, server_status, type ServerState } from '@/helpers/hosting'

/** Hosted servers that are running, in the top bar next to the running instances. */

const { formatMessage } = useVIntl()

const messages = defineMessages({
	viewServer: { id: 'app.hosting.indicator.view', defaultMessage: 'View server' },
	servers: {
		id: 'app.hosting.indicator.servers',
		defaultMessage: '{count} servers running',
	},
})

const running = ref<{ server: HostedServer; state: ServerState }[]>([])
let timer: ReturnType<typeof setInterval> | undefined

async function refresh() {
	try {
		const servers = await list_servers()
		const states = await Promise.all(
			servers.map(async (server) => ({ server, state: (await server_status(server.id)).state })),
		)
		running.value = states.filter((entry) => entry.state !== 'offline')
	} catch {
		running.value = []
	}
}

const link = computed(() =>
	running.value.length === 1 ? `/host/${encodeURIComponent(running.value[0].server.id)}` : '/host',
)
const label = computed(() =>
	running.value.length === 1
		? running.value[0].server.name
		: formatMessage(messages.servers, { count: running.value.length }),
)

onMounted(() => {
	void refresh()
	timer = setInterval(() => void refresh(), 3000)
})
onUnmounted(() => {
	if (timer) clearInterval(timer)
})
</script>

<template>
	<router-link
		v-if="running.length > 0"
		v-tooltip="formatMessage(messages.viewServer)"
		:to="link"
		class="flex items-center gap-2 rounded-xl border border-solid border-surface-5 px-3 py-1.5 text-sm font-medium text-contrast hover:underline"
	>
		<ServerIcon
			class="size-4"
			:class="running.every((entry) => entry.state === 'running') ? 'text-green' : 'text-orange'"
		/>
		{{ label }}
	</router-link>
</template>
