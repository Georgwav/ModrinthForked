<script setup lang="ts">
import {
	CurseForgeIcon,
	DownloadIcon,
	ExternalIcon,
	SearchIcon,
	SpinnerIcon,
} from '@modrinth/assets'
import {
	Button,
	Chips,
	Combobox,
	type ComboboxOption,
	defineMessages,
	EmptyState,
	injectNotificationManager,
	Input,
	useCompactNumber,
	useVIntl,
} from '@modrinth/ui'
import type { GameVersionTag } from '@modrinth/utils'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, onActivated, onMounted, ref, watch } from 'vue'

import CurseForgeVersions from '@/components/ui/curseforge/CurseForgeVersions.vue'
import ExternalProjectCard from '@/components/ui/curseforge/ExternalProjectCard.vue'
import InstallToInstanceModal from '@/components/ui/curseforge/InstallToInstanceModal.vue'
import ManualDownloadsModal from '@/components/ui/curseforge/ManualDownloadsModal.vue'
import {
	curseforge_install_modpack,
	curseforge_search,
	curseforge_status,
	type CurseForgeFile,
	type CurseForgeLoader,
	type CurseForgeModInstall,
	type CurseForgeProject,
	type CurseForgeSort,
	ftb_install_pack,
	ftb_search,
	type FtbPack,
	type FtbVersion,
	type PackInstallReport,
} from '@/helpers/curseforge'
import { get_game_versions } from '@/helpers/tags'
import type { GameInstance } from '@/helpers/types'
import { useRootBreadcrumb } from '@/providers/breadcrumbs'

defineOptions({ name: 'CurseForgePage' })

type Section = 'mod' | 'modpack'
type Source = 'curseforge' | 'ftb'

const PAGE_SIZE = 20

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()
const { formatCompactNumber } = useCompactNumber()

const messages = defineMessages({
	heading: { id: 'app.curseforge.heading', defaultMessage: 'CurseForge' },
	mods: { id: 'app.curseforge.section.mods', defaultMessage: 'Mods' },
	modpacks: { id: 'app.curseforge.section.modpacks', defaultMessage: 'Modpacks' },
	sourceCurseForge: { id: 'app.curseforge.source.curseforge', defaultMessage: 'CurseForge' },
	sourceFtb: { id: 'app.curseforge.source.ftb', defaultMessage: 'Feed the Beast' },
	searchMods: { id: 'app.curseforge.search.mods', defaultMessage: 'Search CurseForge mods…' },
	searchModpacks: {
		id: 'app.curseforge.search.modpacks',
		defaultMessage: 'Search modpacks…',
	},
	allVersions: { id: 'app.curseforge.filter.all-versions', defaultMessage: 'All versions' },
	allLoaders: { id: 'app.curseforge.filter.all-loaders', defaultMessage: 'All loaders' },
	sortPopularity: { id: 'app.curseforge.sort.popularity', defaultMessage: 'Popular' },
	sortUpdated: { id: 'app.curseforge.sort.updated', defaultMessage: 'Recently updated' },
	sortDownloads: { id: 'app.curseforge.sort.downloads', defaultMessage: 'Most downloads' },
	downloads: {
		id: 'app.curseforge.stat.downloads',
		defaultMessage: '{count} downloads',
	},
	installs: { id: 'app.curseforge.stat.installs', defaultMessage: '{count} installs' },
	install: { id: 'app.curseforge.install', defaultMessage: 'Install' },
	downloadManually: {
		id: 'app.curseforge.download-manually',
		defaultMessage: 'Download manually',
	},
	loadMore: { id: 'app.curseforge.load-more', defaultMessage: 'Load more' },
	noResults: { id: 'app.curseforge.no-results', defaultMessage: 'Nothing found' },
	noResultsDescription: {
		id: 'app.curseforge.no-results-description',
		defaultMessage: 'Try other words or fewer filters.',
	},
	notSetUp: {
		id: 'app.curseforge.not-set-up',
		defaultMessage: 'CurseForge isn’t set up in this build',
	},
	notSetUpDescription: {
		id: 'app.curseforge.not-set-up-description',
		defaultMessage:
			'This build of Threadrinth was made without a CurseForge API key, so CurseForge can’t be searched. Feed the Beast modpacks still work.',
	},
	showFtb: { id: 'app.curseforge.show-ftb', defaultMessage: 'Browse Feed the Beast' },
	installingPack: {
		id: 'app.curseforge.installing-pack',
		defaultMessage: 'Installing {name}',
	},
	installingPackDescription: {
		id: 'app.curseforge.installing-pack-description',
		defaultMessage: 'The new instance is being set up; the downloads panel shows the progress.',
	},
	preparingPack: {
		id: 'app.curseforge.preparing-pack',
		defaultMessage: 'Preparing {name}',
	},
	preparingPackDescription: {
		id: 'app.curseforge.preparing-pack-description',
		defaultMessage: 'Reading the modpack’s file list…',
	},
	versionInfo: {
		id: 'app.curseforge.ftb.version-info',
		defaultMessage: 'Minecraft {game} · {loader}',
	},
	noVersions: {
		id: 'app.curseforge.ftb.no-versions',
		defaultMessage: 'This pack has no public versions.',
	},
})

const breadcrumb = useRootBreadcrumb({
	slot: 'root',
	id: 'curseforge',
	label: formatMessage(messages.heading),
	to: '/curseforge',
	visual: { type: 'icon', component: CurseForgeIcon },
})
onActivated(breadcrumb.reset)

const cfAvailable = ref(true)
const section = ref<Section>('mod')
const source = ref<Source>('curseforge')
const query = ref('')
const gameVersion = ref<string>('')
const loader = ref<CurseForgeLoader | ''>('')
const sort = ref<CurseForgeSort>('popularity')

const cfResults = ref<CurseForgeProject[]>([])
const cfTotal = ref(0)
const ftbResults = ref<FtbPack[]>([])
const loading = ref(false)
const expanded = ref<Set<number>>(new Set())
const installingIds = ref<Set<string>>(new Set())
const gameVersions = ref<GameVersionTag[]>([])

const installModal = ref<InstanceType<typeof InstallToInstanceModal>>()
const manualModal = ref<InstanceType<typeof ManualDownloadsModal>>()

const sections = computed<Section[]>(() => ['mod', 'modpack'])
const sources = computed<Source[]>(() => ['curseforge', 'ftb'])
const usingFtb = computed(() => section.value === 'modpack' && source.value === 'ftb')
const blocked = computed(() => !usingFtb.value && !cfAvailable.value)

const gameVersionOptions = computed<ComboboxOption<string>[]>(() => [
	{ value: '', label: formatMessage(messages.allVersions) },
	...gameVersions.value
		.filter((version) => version.version_type === 'release')
		.map((version) => ({ value: version.version, label: version.version })),
])
const loaderOptions = computed<ComboboxOption<CurseForgeLoader | ''>[]>(() => [
	{ value: '', label: formatMessage(messages.allLoaders) },
	{ value: 'forge', label: 'Forge' },
	{ value: 'neoforge', label: 'NeoForge' },
	{ value: 'fabric', label: 'Fabric' },
	{ value: 'quilt', label: 'Quilt' },
])
const sortOptions = computed<ComboboxOption<CurseForgeSort>[]>(() => [
	{ value: 'popularity', label: formatMessage(messages.sortPopularity) },
	{ value: 'updated', label: formatMessage(messages.sortUpdated) },
	{ value: 'downloads', label: formatMessage(messages.sortDownloads) },
])

let searchId = 0
async function search(reset = true) {
	if (blocked.value) {
		cfResults.value = []
		ftbResults.value = []
		return
	}
	const id = ++searchId
	loading.value = true
	try {
		if (usingFtb.value) {
			const packs = await ftb_search({
				query: query.value,
				game_version: gameVersion.value || null,
				loader: loader.value || null,
				limit: 40,
			})
			if (id !== searchId) return
			ftbResults.value = packs
		} else {
			const results = await curseforge_search({
				class: section.value,
				query: query.value,
				game_version: gameVersion.value || null,
				loader: loader.value || null,
				sort: sort.value,
				index: reset ? 0 : cfResults.value.length,
				page_size: PAGE_SIZE,
			})
			if (id !== searchId) return
			cfResults.value = reset ? results.projects : [...cfResults.value, ...results.projects]
			cfTotal.value = results.total
		}
		if (reset) expanded.value = new Set()
	} catch (error) {
		if (id === searchId) handleError(error as Error)
	} finally {
		if (id === searchId) loading.value = false
	}
}

let debounce: ReturnType<typeof setTimeout> | undefined
watch(query, () => {
	clearTimeout(debounce)
	debounce = setTimeout(() => void search(true), 400)
})
watch([section, source], () => {
	cfResults.value = []
	ftbResults.value = []
	void search(true)
})
watch([gameVersion, loader, sort], () => void search(true))

onMounted(async () => {
	try {
		cfAvailable.value = (await curseforge_status()).available
	} catch {
		cfAvailable.value = false
	}
	if (!cfAvailable.value) {
		section.value = 'modpack'
		source.value = 'ftb'
	}
	get_game_versions()
		.then((versions: GameVersionTag[]) => (gameVersions.value = versions))
		.catch(handleError)
	await search(true)
})

function toggle(id: number) {
	const next = new Set(expanded.value)
	if (next.has(id)) next.delete(id)
	else next.add(id)
	expanded.value = next
}

function sectionLabel(value: Section) {
	return formatMessage(value === 'mod' ? messages.mods : messages.modpacks)
}

function sourceLabel(value: Source) {
	return formatMessage(value === 'ftb' ? messages.sourceFtb : messages.sourceCurseForge)
}

function projectStats(project: CurseForgeProject) {
	return [formatMessage(messages.downloads, { count: formatCompactNumber(project.downloads) })]
}

function projectTags(project: CurseForgeProject) {
	return [...project.loaders, ...project.game_versions.slice(0, 3)]
}

function ftbStats(pack: FtbPack) {
	return [formatMessage(messages.installs, { count: formatCompactNumber(pack.installs) })]
}

function ftbTags(pack: FtbPack) {
	const newest = pack.versions[0]
	return [newest?.loader, newest?.game_version, ...pack.tags.slice(0, 2)].filter(
		(tag): tag is string => !!tag,
	)
}

/** The newest release, or else the newest version. */
function defaultFtbVersion(pack: FtbPack): FtbVersion | undefined {
	return pack.versions.find((version) => version.release_type === 'release') ?? pack.versions[0]
}

function onModInstalled(result: CurseForgeModInstall, instance: GameInstance) {
	if (result.manual_downloads.length > 0) {
		manualModal.value?.show(result.manual_downloads, instance.name)
	}
}

function installMod(project: CurseForgeProject, file?: CurseForgeFile) {
	installModal.value?.show(project, file ? { id: file.id, name: file.name } : undefined)
}

async function startPackInstall(
	key: string,
	name: string,
	start: () => Promise<PackInstallReport>,
) {
	const next = new Set(installingIds.value)
	next.add(key)
	installingIds.value = next
	addNotification({
		title: formatMessage(messages.preparingPack, { name }),
		text: formatMessage(messages.preparingPackDescription),
		type: 'info',
	})
	try {
		const report = await start()
		addNotification({
			title: formatMessage(messages.installingPack, { name: report.instance_name }),
			text: formatMessage(messages.installingPackDescription),
			type: 'success',
		})
		if (report.manual_downloads.length > 0) {
			manualModal.value?.show(report.manual_downloads, report.instance_name)
		}
	} catch (error) {
		handleError(error as Error)
	} finally {
		const done = new Set(installingIds.value)
		done.delete(key)
		installingIds.value = done
	}
}

function installCurseForgePack(project: CurseForgeProject, file?: CurseForgeFile) {
	return startPackInstall(`cf-${project.id}`, project.name, () =>
		curseforge_install_modpack(project.id, file?.id),
	)
}

function installFtbPack(pack: FtbPack, version?: FtbVersion) {
	const chosen = version ?? defaultFtbVersion(pack)
	if (!chosen) return
	return startPackInstall(`ftb-${pack.id}`, pack.name, () => ftb_install_pack(pack.id, chosen.id))
}

function onInstallClick(project: CurseForgeProject, file?: CurseForgeFile) {
	if (section.value === 'mod') installMod(project, file)
	else void installCurseForgePack(project, file)
}

function ftbVersionInfo(version: FtbVersion) {
	return formatMessage(messages.versionInfo, {
		game: version.game_version ?? '?',
		loader: [version.loader, version.loader_version].filter(Boolean).join(' ') || 'vanilla',
	})
}

function openWebsite(url: string | null) {
	if (url) void openUrl(url)
}

function formatUnixDate(seconds: number) {
	return seconds > 0 ? new Date(seconds * 1000).toLocaleDateString() : ''
}

const hasMoreCf = computed(() => !usingFtb.value && cfResults.value.length < cfTotal.value)
const empty = computed(
	() =>
		!loading.value &&
		!blocked.value &&
		(usingFtb.value ? ftbResults.value.length === 0 : cfResults.value.length === 0),
)
</script>

<template>
	<div class="flex flex-col gap-4 p-6">
		<div class="flex flex-wrap items-center gap-4">
			<h1 class="m-0 flex items-center gap-2 text-2xl font-extrabold text-contrast">
				<CurseForgeIcon class="size-7" />
				{{ formatMessage(messages.heading) }}
			</h1>
			<Chips v-model="section" :items="sections" :format-label="sectionLabel" :capitalize="false" />
			<Chips
				v-if="section === 'modpack'"
				v-model="source"
				:items="sources"
				:format-label="sourceLabel"
				:capitalize="false"
			/>
		</div>

		<EmptyState
			v-if="blocked"
			:heading="formatMessage(messages.notSetUp)"
			:description="formatMessage(messages.notSetUpDescription)"
		>
			<template #actions>
				<Button
					type="colored"
					color="brand"
					@click="
						() => {
							section = 'modpack'
							source = 'ftb'
						}
					"
				>
					{{ formatMessage(messages.showFtb) }}
				</Button>
			</template>
		</EmptyState>

		<template v-else>
			<div class="flex flex-wrap items-center gap-2">
				<Input
					v-model="query"
					:icon="SearchIcon"
					type="text"
					:placeholder="
						formatMessage(section === 'mod' ? messages.searchMods : messages.searchModpacks)
					"
					clearable
					wrapper-class="min-w-[16rem] flex-1"
				/>
				<Combobox
					v-model="gameVersion"
					class="w-44"
					:options="gameVersionOptions"
					searchable
					sync-with-selection
				/>
				<Combobox v-model="loader" class="w-40" :options="loaderOptions" />
				<Combobox v-if="!usingFtb" v-model="sort" class="w-48" :options="sortOptions" />
			</div>

			<div
				v-if="loading && cfResults.length === 0 && ftbResults.length === 0"
				class="flex justify-center p-8"
			>
				<SpinnerIcon class="size-8 animate-spin text-secondary" />
			</div>
			<EmptyState
				v-else-if="empty"
				:heading="formatMessage(messages.noResults)"
				:description="formatMessage(messages.noResultsDescription)"
			/>

			<div v-if="usingFtb" class="flex flex-col gap-3">
				<ExternalProjectCard
					v-for="pack in ftbResults"
					:key="`ftb-${pack.id}`"
					:name="pack.name"
					:summary="pack.summary"
					:icon-url="pack.icon_url"
					:authors="pack.authors"
					:stats="ftbStats(pack)"
					:tags="ftbTags(pack)"
					:website-url="pack.website_url"
					:expanded="expanded.has(pack.id)"
					@toggle="toggle(pack.id)"
				>
					<template #actions>
						<Button
							type="colored"
							color="brand"
							:disabled="installingIds.has(`ftb-${pack.id}`) || pack.versions.length === 0"
							:loading="installingIds.has(`ftb-${pack.id}`)"
							@click="installFtbPack(pack)"
						>
							<DownloadIcon />
							{{ formatMessage(messages.install) }}
						</Button>
					</template>
					<div class="flex flex-col gap-2">
						<p v-if="pack.versions.length === 0" class="m-0 text-secondary">
							{{ formatMessage(messages.noVersions) }}
						</p>
						<div
							v-for="version in pack.versions"
							:key="version.id"
							class="flex items-center justify-between gap-4 rounded-xl bg-bg p-3"
						>
							<div class="flex min-w-0 flex-col gap-1">
								<span class="truncate font-semibold text-contrast">
									{{ version.name }}
									<span class="text-sm font-normal text-secondary">
										({{ version.release_type }})
									</span>
								</span>
								<span class="text-sm text-secondary">
									{{ ftbVersionInfo(version) }} · {{ formatUnixDate(version.updated) }}
								</span>
							</div>
							<Button
								size="sm"
								:disabled="installingIds.has(`ftb-${pack.id}`)"
								@click="installFtbPack(pack, version)"
							>
								<DownloadIcon />
								{{ formatMessage(messages.install) }}
							</Button>
						</div>
					</div>
				</ExternalProjectCard>
			</div>

			<div v-else class="flex flex-col gap-3">
				<ExternalProjectCard
					v-for="project in cfResults"
					:key="`cf-${project.id}`"
					:name="project.name"
					:summary="project.summary"
					:icon-url="project.icon_url"
					:authors="project.authors"
					:stats="projectStats(project)"
					:tags="projectTags(project)"
					:website-url="project.website_url"
					:expanded="expanded.has(project.id)"
					@toggle="toggle(project.id)"
				>
					<template #actions>
						<Button
							v-if="project.allow_distribution"
							type="colored"
							color="brand"
							:disabled="installingIds.has(`cf-${project.id}`)"
							:loading="installingIds.has(`cf-${project.id}`)"
							@click="onInstallClick(project)"
						>
							<DownloadIcon />
							{{ formatMessage(messages.install) }}
						</Button>
						<Button v-else-if="project.website_url" @click="openWebsite(project.website_url)">
							<ExternalIcon />
							{{ formatMessage(messages.downloadManually) }}
						</Button>
					</template>
					<CurseForgeVersions
						:project-id="project.id"
						:game-version="gameVersion || null"
						:loader="loader || null"
						:installing="installingIds.has(`cf-${project.id}`)"
						@install="(file) => onInstallClick(project, file)"
					/>
				</ExternalProjectCard>
				<div v-if="hasMoreCf" class="flex justify-center">
					<Button :loading="loading" :disabled="loading" @click="search(false)">
						{{ formatMessage(messages.loadMore) }}
					</Button>
				</div>
			</div>
		</template>

		<InstallToInstanceModal ref="installModal" @installed="onModInstalled" />
		<ManualDownloadsModal ref="manualModal" />
	</div>
</template>
