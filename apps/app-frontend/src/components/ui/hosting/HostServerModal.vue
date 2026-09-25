<script setup lang="ts">
import { ServerIcon, SpinnerIcon, XIcon } from '@modrinth/assets'
import {
	Admonition,
	Button,
	Checkbox,
	Chips,
	Combobox,
	type ComboboxOption,
	commonMessages,
	defineMessages,
	FileTreeSelect,
	injectNotificationManager,
	Input,
	NewModal,
	Slider,
	useVIntl,
} from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, ref, shallowRef } from 'vue'
import { useRouter } from 'vue-router'

import { create_server } from '@/helpers/hosting'
import { get_pack_export_candidates, list, type PackExportCandidate } from '@/helpers/instance'
import { server_pack_selection } from '@/helpers/threadrinth'
import type { GameInstance } from '@/helpers/types'
import { getWorldDisplayName, refreshWorlds, type SingleplayerWorld } from '@/helpers/worlds'

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()
const router = useRouter()

const emit = defineEmits<{
	created: [id: string]
}>()

const messages = defineMessages({
	header: { id: 'app.hosting.create.header', defaultMessage: 'Host a server' },
	instance: { id: 'app.hosting.create.instance', defaultMessage: 'Instance' },
	instancePlaceholder: {
		id: 'app.hosting.create.instance-placeholder',
		defaultMessage: 'Choose an instance',
	},
	name: { id: 'app.hosting.create.name', defaultMessage: 'Server name' },
	files: { id: 'app.hosting.create.files', defaultMessage: 'Files' },
	filesDescription: {
		id: 'app.hosting.create.files-description',
		defaultMessage:
			'Mods Modrinth lists as client-only start unchecked. The mod loader and Java are set up for you.',
	},
	world: { id: 'app.hosting.create.world', defaultMessage: 'World' },
	newWorld: { id: 'app.hosting.create.new-world', defaultMessage: 'New world' },
	copyWorld: { id: 'app.hosting.create.copy-world', defaultMessage: 'Copy a world' },
	seed: { id: 'app.hosting.create.seed', defaultMessage: 'Seed (optional)' },
	worldPlaceholder: {
		id: 'app.hosting.create.world-placeholder',
		defaultMessage: 'Choose a world',
	},
	noWorlds: {
		id: 'app.hosting.create.no-worlds',
		defaultMessage: 'This instance has no singleplayer worlds.',
	},
	memory: { id: 'app.hosting.create.memory', defaultMessage: 'Memory' },
	eula: {
		id: 'app.hosting.create.eula',
		defaultMessage: 'I accept the Minecraft EULA',
	},
	readEula: { id: 'app.hosting.create.read-eula', defaultMessage: 'Read the EULA' },
	create: { id: 'app.hosting.create.button', defaultMessage: 'Create server' },
	creating: {
		id: 'app.hosting.create.creating',
		defaultMessage:
			'Setting up the server. The first time downloads the server and Java, which can take a few minutes.',
	},
})

type WorldChoice = 'new' | 'copy'

const modal = ref<InstanceType<typeof NewModal>>()
const instances = ref<GameInstance[]>([])
const instanceId = ref<string>()
const fixedInstance = ref(false)
const name = ref<string | number | undefined>('')
const files = shallowRef<PackExportCandidate[]>([])
const includedPaths = ref<string[]>([])
const excludedPaths = ref<string[]>([])
const fileTreeKey = ref(0)
const directoryEntries = new Map<string, PackExportCandidate[]>()
const currentDirectory = ref('')
const loadId = ref(0)
const worldChoice = ref<WorldChoice>('new')
const seed = ref<string | number | undefined>('')
const worldSourceInstanceId = ref<string>()
const worlds = ref<SingleplayerWorld[]>([])
const worldPath = ref<string>()
const memoryMb = ref(4096)
const eulaAccepted = ref(false)
const creating = ref(false)

const instance = computed(() => instances.value.find((other) => other.id === instanceId.value))
const instanceOptions = computed<ComboboxOption<string>[]>(() =>
	instances.value
		.filter((other) => !other.quarantined)
		.sort((a, b) => a.name.localeCompare(b.name))
		.map((other) => ({
			value: other.id,
			label: other.name,
			subLabel: `${other.loader} ${other.game_version}`,
		})),
)
const worldOptions = computed<ComboboxOption<string>[]>(() =>
	worlds.value.map((world) => ({ value: world.path, label: getWorldDisplayName(world) })),
)
const canCreate = computed(
	() =>
		!!instance.value &&
		String(name.value ?? '').trim().length > 0 &&
		(worldChoice.value === 'new' || !!worldPath.value) &&
		!creating.value,
)

async function show(selected?: GameInstance) {
	instances.value = await list().catch((error) => {
		handleError(error)
		return []
	})
	fixedInstance.value = !!selected
	worldChoice.value = 'new'
	seed.value = ''
	worldPath.value = undefined
	memoryMb.value = 4096
	eulaAccepted.value = false
	creating.value = false
	await selectInstance(selected?.id)
	modal.value?.show()
}

async function selectInstance(id: string | undefined) {
	instanceId.value = id
	worldSourceInstanceId.value = id
	files.value = []
	includedPaths.value = []
	excludedPaths.value = []
	directoryEntries.clear()
	currentDirectory.value = ''
	fileTreeKey.value += 1
	if (!id) return
	name.value = `${instance.value?.name ?? ''} Server`
	void loadWorlds(id)
	const load = ++loadId.value
	try {
		const [candidates, selection] = await Promise.all([
			get_pack_export_candidates(id),
			server_pack_selection(id),
		])
		if (load !== loadId.value) return
		directoryEntries.set('', candidates)
		files.value = candidates
		includedPaths.value = selection.included
		excludedPaths.value = selection.excluded
		fileTreeKey.value += 1
	} catch (error) {
		handleError(error as Error)
	}
}

async function loadWorlds(id: string) {
	worldPath.value = undefined
	const all = await refreshWorlds(id)
	worlds.value = all.filter((world): world is SingleplayerWorld => world.type === 'singleplayer')
}

async function loadDirectory(path: string) {
	const id = instanceId.value
	if (!id) return
	const normalized = path.replaceAll('\\', '/').split('/').filter(Boolean).join('/')
	currentDirectory.value = normalized
	const cached = directoryEntries.get(normalized)
	if (cached) {
		files.value = cached
		return
	}
	const load = loadId.value
	files.value = []
	try {
		const children = await get_pack_export_candidates(id, normalized || undefined)
		if (load !== loadId.value) return
		directoryEntries.set(normalized, children)
		if (currentDirectory.value === normalized) files.value = children
	} catch {
		if (currentDirectory.value === normalized) files.value = []
	}
}

async function create() {
	const id = instanceId.value
	if (!id || !canCreate.value) return
	creating.value = true
	try {
		const server = await create_server({
			instance_id: id,
			name: String(name.value ?? '').trim(),
			included: includedPaths.value,
			excluded: excludedPaths.value,
			world:
				worldChoice.value === 'copy' && worldSourceInstanceId.value && worldPath.value
					? { type: 'copy', instance_id: worldSourceInstanceId.value, world: worldPath.value }
					: { type: 'new', seed: String(seed.value ?? '').trim() || null },
			memory_mb: memoryMb.value,
			eula_accepted: eulaAccepted.value,
		})
		modal.value?.hide()
		emit('created', server.id)
		await router.push(`/host/${encodeURIComponent(server.id)}`)
	} catch (error) {
		handleError(error as Error)
	} finally {
		creating.value = false
	}
}

defineExpose({ show })
</script>

<template>
	<NewModal
		ref="modal"
		:header="formatMessage(messages.header)"
		scrollable
		width="46rem"
		max-width="calc(100vw - 2rem)"
	>
		<div class="flex flex-col gap-4">
			<label v-if="!fixedInstance" class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">{{ formatMessage(messages.instance) }}</span>
				<Combobox
					:model-value="instanceId"
					:options="instanceOptions"
					:placeholder="formatMessage(messages.instancePlaceholder)"
					searchable
					sync-with-selection
					@update:model-value="(value) => selectInstance(value as string | undefined)"
				/>
			</label>
			<template v-if="instance">
				<label class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">{{ formatMessage(messages.name) }}</span>
					<Input v-model="name" type="text" wrapper-class="w-full" />
				</label>
				<div class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">{{ formatMessage(messages.world) }}</span>
					<Chips
						v-model="worldChoice"
						:items="['new', 'copy'] as WorldChoice[]"
						:capitalize="false"
						:format-label="
							(item: WorldChoice) =>
								formatMessage(item === 'new' ? messages.newWorld : messages.copyWorld)
						"
					/>
					<Input
						v-if="worldChoice === 'new'"
						v-model="seed"
						type="text"
						:placeholder="formatMessage(messages.seed)"
						wrapper-class="w-full"
					/>
					<template v-else>
						<Combobox
							:model-value="worldSourceInstanceId"
							:options="instanceOptions"
							searchable
							sync-with-selection
							@update:model-value="
								(value) => {
									worldSourceInstanceId = value as string | undefined
									if (value) void loadWorlds(value as string)
								}
							"
						/>
						<Combobox
							v-if="worldOptions.length > 0"
							v-model="worldPath"
							:options="worldOptions"
							:placeholder="formatMessage(messages.worldPlaceholder)"
							sync-with-selection
						/>
						<p v-else class="m-0 text-secondary">{{ formatMessage(messages.noWorlds) }}</p>
					</template>
				</div>
				<div class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">{{ formatMessage(messages.memory) }}</span>
					<Slider v-model="memoryMb" :min="1024" :max="16384" :step="512" unit="MB" />
				</div>
				<div class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">{{ formatMessage(messages.files) }}</span>
					<p class="m-0 text-sm text-secondary">{{ formatMessage(messages.filesDescription) }}</p>
					<FileTreeSelect
						:key="fileTreeKey"
						v-model="includedPaths"
						v-model:excluded-paths="excludedPaths"
						class="min-w-0"
						:items="files"
						lazy
						@navigate="loadDirectory"
					/>
				</div>
				<div class="flex flex-wrap items-center gap-3">
					<Checkbox v-model="eulaAccepted" :label="formatMessage(messages.eula)" />
					<button
						class="border-none bg-transparent p-0 text-link underline"
						@click="openUrl('https://aka.ms/MinecraftEULA')"
					>
						{{ formatMessage(messages.readEula) }}
					</button>
				</div>
				<Admonition v-if="creating" type="info" :header="formatMessage(messages.creating)" />
			</template>
		</div>
		<template #actions>
			<div class="flex items-center justify-end gap-2">
				<Button type="outlined" :disabled="creating" @click="modal?.hide()">
					<XIcon />
					{{ formatMessage(commonMessages.cancelButton) }}
				</Button>
				<Button type="colored" color="brand" :disabled="!canCreate" @click="create">
					<SpinnerIcon v-if="creating" class="animate-spin" />
					<ServerIcon v-else />
					{{ formatMessage(messages.create) }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>
