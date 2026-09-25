<script setup lang="ts">
import { DownloadIcon, ExternalIcon, SpinnerIcon } from '@modrinth/assets'
import { Button, defineMessages, injectNotificationManager, TagItem, useVIntl } from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, onMounted, ref } from 'vue'

import { curseforge_files, type CurseForgeFile } from '@/helpers/curseforge'

const { formatMessage } = useVIntl()
const { handleError } = injectNotificationManager()

const props = defineProps<{
	projectId: number
	gameVersion: string | null
	loader: string | null
	installing: boolean
}>()

const emit = defineEmits<{
	install: [file: CurseForgeFile]
}>()

const messages = defineMessages({
	loading: { id: 'app.curseforge.versions.loading', defaultMessage: 'Loading versions…' },
	none: {
		id: 'app.curseforge.versions.none',
		defaultMessage: 'No versions match the selected Minecraft version and loader.',
	},
	install: { id: 'app.curseforge.versions.install', defaultMessage: 'Install' },
	downloadManually: {
		id: 'app.curseforge.versions.download-manually',
		defaultMessage: 'Download manually',
	},
	loadMore: { id: 'app.curseforge.versions.load-more', defaultMessage: 'Load more' },
})

const files = ref<CurseForgeFile[]>([])
const total = ref(0)
const nextIndex = ref(0)
const loading = ref(false)
const hasMore = computed(() => nextIndex.value < total.value)
/** The API pages files 50 at a time. */
const PAGE_SIZE = 50

async function load(reset: boolean) {
	loading.value = true
	try {
		const page = await curseforge_files(
			props.projectId,
			props.gameVersion,
			props.loader,
			reset ? 0 : nextIndex.value,
		)
		files.value = reset ? page.files : [...files.value, ...page.files]
		total.value = page.total
		nextIndex.value = page.index + PAGE_SIZE
	} catch (error) {
		handleError(error as Error)
	} finally {
		loading.value = false
	}
}

function formatDate(date: string) {
	const parsed = new Date(date)
	return Number.isNaN(parsed.getTime()) ? '' : parsed.toLocaleDateString()
}

onMounted(() => load(true))
</script>

<template>
	<div class="flex flex-col gap-2">
		<div v-if="loading && files.length === 0" class="flex items-center gap-2 text-secondary">
			<SpinnerIcon class="animate-spin" />
			{{ formatMessage(messages.loading) }}
		</div>
		<p v-else-if="files.length === 0" class="m-0 text-secondary">
			{{ formatMessage(messages.none) }}
		</p>
		<div
			v-for="file in files"
			:key="file.id"
			class="flex items-center justify-between gap-4 rounded-xl bg-bg p-3"
		>
			<div class="flex min-w-0 flex-col gap-1">
				<span class="truncate font-semibold text-contrast">{{ file.name }}</span>
				<div class="flex flex-wrap items-center gap-1 text-sm text-secondary">
					<TagItem>{{ file.release_type }}</TagItem>
					<TagItem v-for="loaderName in file.loaders" :key="loaderName">{{ loaderName }}</TagItem>
					<span>{{ file.game_versions.slice(0, 4).join(', ') }}</span>
					<span v-if="file.game_versions.length > 4">…</span>
					<span>· {{ formatDate(file.date) }}</span>
				</div>
			</div>
			<Button
				v-if="file.downloadable"
				size="sm"
				:disabled="installing"
				@click="emit('install', file)"
			>
				<DownloadIcon />
				{{ formatMessage(messages.install) }}
			</Button>
			<Button v-else size="sm" @click="openUrl(file.page_url)">
				<ExternalIcon />
				{{ formatMessage(messages.downloadManually) }}
			</Button>
		</div>
		<div v-if="hasMore" class="flex justify-center">
			<Button size="sm" :loading="loading" :disabled="loading" @click="load(false)">
				{{ formatMessage(messages.loadMore) }}
			</Button>
		</div>
	</div>
</template>
