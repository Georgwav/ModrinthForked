<script setup lang="ts">
import { FolderOpenIcon, ServerIcon, XIcon } from '@modrinth/assets'
import {
	Button,
	commonMessages,
	defineMessages,
	FileTreeSelect,
	injectNotificationManager,
	injectPopupNotificationManager,
	NewModal,
	useVIntl,
} from '@modrinth/ui'
import { save } from '@tauri-apps/plugin-dialog'
import { ref, shallowRef } from 'vue'

import { get_pack_export_candidates, type PackExportCandidate } from '@/helpers/instance'
import { export_server_pack, server_pack_selection } from '@/helpers/threadrinth'
import type { GameInstance } from '@/helpers/types'
import { highlightInFolder } from '@/helpers/utils'

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()
const popupNotificationManager = injectPopupNotificationManager()

const props = defineProps<{
	instance: GameInstance
}>()

const messages = defineMessages({
	header: { id: 'app.server-pack.header', defaultMessage: 'Export server pack' },
	description: {
		id: 'app.server-pack.description',
		defaultMessage:
			'Pick what the server gets. Mods Modrinth lists as client-only start unchecked; the mod loader and start scripts are added for you.',
	},
	exportButton: { id: 'app.server-pack.export-button', defaultMessage: 'Export' },
	exporting: { id: 'app.server-pack.exporting', defaultMessage: 'Exporting server pack' },
	exportingDescription: {
		id: 'app.server-pack.exporting-description',
		defaultMessage: 'Collecting the files and downloading the server launcher for {name}.',
	},
	exported: { id: 'app.server-pack.exported', defaultMessage: 'Server pack exported' },
	exportedDescription: {
		id: 'app.server-pack.exported-description',
		defaultMessage:
			'{count, plural, one {# mod} other {# mods}} included. Run start.sh or start.bat to start the server; README.txt explains the rest.',
	},
	zipFiles: { id: 'app.server-pack.zip-files', defaultMessage: 'Zip archive' },
})

const modal = ref<InstanceType<typeof NewModal>>()
const files = shallowRef<PackExportCandidate[]>([])
const includedPaths = ref<string[]>([])
const excludedPaths = ref<string[]>([])
const fileTreeKey = ref(0)
const loadId = ref(0)
const loading = ref(false)
const directoryEntries = new Map<string, PackExportCandidate[]>()
const currentDirectory = ref('')

function show() {
	files.value = []
	includedPaths.value = []
	excludedPaths.value = []
	fileTreeKey.value += 1
	directoryEntries.clear()
	currentDirectory.value = ''
	modal.value?.show()
	void loadFiles().catch(handleError)
}

async function loadFiles() {
	const id = ++loadId.value
	loading.value = true
	try {
		const [candidates, selection] = await Promise.all([
			get_pack_export_candidates(props.instance.id),
			server_pack_selection(props.instance.id),
		])
		if (id !== loadId.value) return
		directoryEntries.set('', candidates)
		files.value = candidates
		includedPaths.value = selection.included
		excludedPaths.value = selection.excluded
		fileTreeKey.value += 1
	} finally {
		if (id === loadId.value) loading.value = false
	}
}

async function loadDirectory(path: string) {
	const normalized = path.replaceAll('\\', '/').split('/').filter(Boolean).join('/')
	currentDirectory.value = normalized
	const cached = directoryEntries.get(normalized)
	if (cached) {
		files.value = cached
		return
	}
	const id = loadId.value
	files.value = []
	try {
		const children = await get_pack_export_candidates(props.instance.id, normalized || undefined)
		if (id !== loadId.value) return
		directoryEntries.set(normalized, children)
		if (currentDirectory.value === normalized) files.value = children
	} catch {
		if (currentDirectory.value === normalized) files.value = []
	}
}

async function exportPack() {
	const outputPath = await save({
		defaultPath: `${props.instance.name} server.zip`,
		filters: [{ name: formatMessage(messages.zipFiles), extensions: ['zip'] }],
	})
	if (!outputPath) return
	modal.value?.hide()

	addNotification({
		title: formatMessage(messages.exporting),
		text: formatMessage(messages.exportingDescription, { name: props.instance.name }),
		type: 'info',
	})
	try {
		const report = await export_server_pack(
			props.instance.id,
			outputPath,
			includedPaths.value,
			excludedPaths.value,
		)
		popupNotificationManager.addPopupNotification({
			title: formatMessage(messages.exported),
			text: formatMessage(messages.exportedDescription, { count: report.mods_included }),
			type: 'success',
			buttons: [
				{
					label: formatMessage(commonMessages.openInFolderButton),
					icon: FolderOpenIcon,
					action: () => highlightInFolder(outputPath).catch(handleError),
				},
			],
		})
	} catch (error) {
		handleError(error as Error)
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
			<p class="m-0 text-secondary">{{ formatMessage(messages.description) }}</p>
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
		<template #actions>
			<div class="flex items-center justify-end gap-2">
				<Button type="outlined" @click="modal?.hide()">
					<XIcon />
					{{ formatMessage(commonMessages.cancelButton) }}
				</Button>
				<Button type="colored" color="brand" :disabled="loading" @click="exportPack">
					<ServerIcon />
					{{ formatMessage(messages.exportButton) }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>
