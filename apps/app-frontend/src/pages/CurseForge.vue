<script setup lang="ts">
import type { Labrinth } from '@modrinth/api-client'
import { CurseForgeIcon, DownloadIcon, ExternalIcon } from '@modrinth/assets'
import type { BrowseSearchResponse, BrowseSearchState, CardAction, ProjectType } from '@modrinth/ui'
import {
	Admonition,
	BrowsePageLayout,
	BrowseSidebar,
	defineMessages,
	injectNotificationManager,
	provideBrowseManager,
	useBrowseSearch,
	useVIntl,
} from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, onActivated, ref, shallowRef, watch } from 'vue'
import { useRoute, useRouter } from 'vue-router'

import InstallToInstanceModal from '@/components/ui/curseforge/InstallToInstanceModal.vue'
import ManualDownloadsModal from '@/components/ui/curseforge/ManualDownloadsModal.vue'
import {
	curseforge_install_modpack,
	curseforge_search,
	curseforge_status,
	type CurseForgeClass,
	type CurseForgeModInstall,
	type CurseForgeProject,
	type CurseForgeSort,
	ftb_install_pack,
	ftb_search,
	type FtbPack,
	type PackInstallReport,
} from '@/helpers/curseforge'
import { get_game_versions, get_loaders } from '@/helpers/tags'
import type { GameInstance } from '@/helpers/types'
import { useRootBreadcrumb } from '@/providers/breadcrumbs'

/**
 * CurseForge and Feed the Beast, laid out like Discover: the same search,
 * sorting, pages, cards and filter sidebar, with CurseForge's results.
 */

defineOptions({ name: 'CurseForgePage' })

type CurseForgeTab = 'modpack' | 'ftb' | 'mod' | 'resourcepack' | 'datapack' | 'shader'

const TABS: CurseForgeTab[] = ['modpack', 'ftb', 'mod', 'resourcepack', 'datapack', 'shader']
const CLASSES: Record<Exclude<CurseForgeTab, 'ftb'>, CurseForgeClass> = {
	modpack: 'modpack',
	mod: 'mod',
	resourcepack: 'resource_pack',
	datapack: 'data_pack',
	shader: 'shader',
}
/** The loaders CurseForge knows. */
const CURSEFORGE_LOADERS = ['forge', 'neoforge', 'fabric', 'quilt']
/** CurseForge pages no further than this. */
const MAX_RESULTS = 10_000

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()
const route = useRoute()
const router = useRouter()

const messages = defineMessages({
	heading: { id: 'app.curseforge.heading', defaultMessage: 'CurseForge' },
	modpacks: { id: 'app.curseforge.tab.modpacks', defaultMessage: 'Modpacks' },
	ftb: { id: 'app.curseforge.tab.ftb', defaultMessage: 'FTB Modpacks' },
	mods: { id: 'app.curseforge.tab.mods', defaultMessage: 'Mods' },
	resourcePacks: { id: 'app.curseforge.tab.resource-packs', defaultMessage: 'Resource Packs' },
	dataPacks: { id: 'app.curseforge.tab.data-packs', defaultMessage: 'Data Packs' },
	shaders: { id: 'app.curseforge.tab.shaders', defaultMessage: 'Shaders' },
	install: { id: 'app.curseforge.install', defaultMessage: 'Install' },
	downloadManually: {
		id: 'app.curseforge.download-manually',
		defaultMessage: 'Download manually',
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
	installingPack: { id: 'app.curseforge.installing-pack', defaultMessage: 'Installing {name}' },
	installingPackDescription: {
		id: 'app.curseforge.installing-pack-description',
		defaultMessage: 'The new instance is being set up; the downloads panel shows the progress.',
	},
	preparingPack: { id: 'app.curseforge.preparing-pack', defaultMessage: 'Preparing {name}' },
	preparingPackDescription: {
		id: 'app.curseforge.preparing-pack-description',
		defaultMessage: 'Reading the modpack’s file list…',
	},
})

const breadcrumb = useRootBreadcrumb({
	slot: 'root',
	id: 'curseforge',
	label: formatMessage(messages.heading),
	to: '/curseforge/modpack',
	visual: { type: 'icon', component: CurseForgeIcon },
})
onActivated(breadcrumb.reset)

const pageActive = computed(() => route.name === 'CurseForge')
const tab = computed<CurseForgeTab>(() => {
	const value = route.params.projectType as CurseForgeTab
	return TABS.includes(value) ? value : 'modpack'
})
/** The Discover project type whose filters and wording the tab uses. */
function browseTypeOf(value: CurseForgeTab): ProjectType {
	return value === 'ftb' ? 'modpack' : value
}
const projectType = ref<ProjectType>(browseTypeOf(tab.value))

const cfAvailable = ref(true)
const statusChecked = curseforge_status()
	.then((status) => (cfAvailable.value = status.available))
	.catch(() => (cfAvailable.value = false))
watch(
	[cfAvailable, tab, pageActive],
	() => {
		if (pageActive.value && !cfAvailable.value && tab.value !== 'ftb') {
			void router.replace('/curseforge/ftb')
		}
	},
	{ immediate: true },
)

const tags = ref<{
	gameVersions: Labrinth.Tags.v2.GameVersion[]
	loaders: Labrinth.Tags.v2.Loader[]
	categories: Labrinth.Tags.v2.Category[]
}>({ gameVersions: [], loaders: [], categories: [] })
Promise.all([get_game_versions(), get_loaders()])
	.then(([gameVersions, loaders]) => {
		tags.value = {
			gameVersions,
			loaders: (loaders as Labrinth.Tags.v2.Loader[]).filter((loader) =>
				CURSEFORGE_LOADERS.includes(loader.name),
			),
			categories: [],
		}
	})
	.catch(handleError)

type Hit = BrowseSearchResponse['projectHits'][number]
const cfProjects = shallowRef(new Map<string, CurseForgeProject>())
const ftbPacks = shallowRef(new Map<string, FtbPack>())
const installing = ref(new Set<string>())

function isoFromUnix(seconds: number) {
	return seconds > 0 ? new Date(seconds * 1000).toISOString() : ''
}

function hit(
	fields: Pick<Hit, 'project_id' | 'slug' | 'name' | 'summary' | 'downloads' | 'icon_url'> & {
		author: string
		categories: string[]
		loaders: string[]
		created: string
		modified: string
		gallery: string[]
	},
): Hit {
	return {
		project_id: fields.project_id,
		project_types: [projectType.value],
		all_project_types: [projectType.value],
		slug: fields.slug,
		author: fields.author,
		author_id: null,
		organization: null,
		organization_id: null,
		name: fields.name,
		summary: fields.summary,
		categories: fields.categories,
		display_categories: fields.categories,
		downloads: fields.downloads,
		// CurseForge and FTB have no followers; leaving it out hides the stat.
		follows: undefined as unknown as number,
		icon_url: fields.icon_url,
		date_created: fields.created,
		date_modified: fields.modified,
		license: '',
		gallery: fields.gallery,
		featured_gallery: fields.gallery[0] ?? null,
		color: null,
		loaders: fields.loaders,
		disclosure_types: [],
	}
}

function cfHit(project: CurseForgeProject): Hit {
	return hit({
		project_id: String(project.id),
		slug: project.slug,
		name: project.name,
		summary: project.summary,
		downloads: project.downloads,
		icon_url: project.icon_url,
		author: project.authors[0] ?? '',
		categories: project.categories,
		loaders: project.loaders,
		created: project.created ?? project.updated ?? '',
		modified: project.updated ?? '',
		gallery: project.gallery,
	})
}

function ftbHit(pack: FtbPack): Hit {
	const newest = pack.versions[0]
	return hit({
		project_id: String(pack.id),
		slug: null,
		name: pack.name,
		summary: pack.summary,
		downloads: pack.installs,
		icon_url: pack.icon_url,
		author: pack.authors[0] ?? 'Feed the Beast',
		categories: pack.tags,
		loaders: newest?.loader ? [newest.loader] : [],
		created: isoFromUnix(pack.released || pack.updated),
		modified: isoFromUnix(pack.updated),
		gallery: pack.banner_url ? [pack.banner_url] : [],
	})
}

const SORTS: Record<string, CurseForgeSort> = {
	relevance: 'popularity',
	downloads: 'downloads',
	follows: 'rating',
	newest: 'newest',
	updated: 'updated',
}

/** Feed the Beast has no paging, so a search is kept and paged here. */
const ftbCache = new Map<string, FtbPack[]>()

// Assigned right below; searches only run after that.
// eslint-disable-next-line prefer-const
let searchState: BrowseSearchState

function selected(type: string) {
	return searchState.currentFilters.value
		.filter((filter) => filter.type === type && !filter.negative)
		.map((filter) => filter.option)
}

async function search(): Promise<BrowseSearchResponse> {
	const perPage = searchState.maxResults.value
	const offset = (searchState.currentPage.value - 1) * perPage
	const gameVersion = selected('game_version')[0] ?? null
	const loader = selected('mod_loader')[0] ?? null
	const sort = searchState.effectiveCurrentSortType.value.name
	const current = tab.value

	if (current === 'ftb') {
		const key = JSON.stringify([searchState.query.value.trim(), gameVersion, loader])
		let packs = ftbCache.get(key)
		if (!packs) {
			packs = await ftb_search({
				query: searchState.query.value,
				game_version: gameVersion,
				loader,
				limit: 50,
			})
			ftbCache.set(key, packs)
		}
		const sorted = [...packs]
		if (sort === 'downloads') sorted.sort((a, b) => b.installs - a.installs)
		else if (sort === 'follows') sorted.sort((a, b) => b.plays - a.plays)
		else if (sort === 'newest') sorted.sort((a, b) => b.released - a.released)
		else if (sort === 'updated') sorted.sort((a, b) => b.updated - a.updated)
		const page = sorted.slice(offset, offset + perPage)
		ftbPacks.value = new Map(page.map((pack) => [String(pack.id), pack]))
		return {
			projectHits: page.map(ftbHit),
			serverHits: [],
			total_hits: sorted.length,
			per_page: perPage,
		}
	}

	await statusChecked
	if (!cfAvailable.value) {
		return { projectHits: [], serverHits: [], total_hits: 0, per_page: perPage }
	}
	const results = await curseforge_search({
		class: CLASSES[current],
		query: searchState.query.value,
		game_version: gameVersion,
		loader: current === 'mod' || current === 'modpack' ? loader : null,
		sort: SORTS[sort] ?? 'popularity',
		index: Math.min(offset, MAX_RESULTS - perPage),
		page_size: perPage,
	})
	cfProjects.value = new Map(results.projects.map((project) => [String(project.id), project]))
	return {
		projectHits: results.projects.map(cfHit),
		serverHits: [],
		total_hits: Number(results.total),
		per_page: perPage,
	}
}

async function searchWithErrors(): Promise<BrowseSearchResponse> {
	try {
		return await search()
	} catch (error) {
		handleError(error as Error)
		throw error
	}
}

searchState = useBrowseSearch({
	projectType,
	tags,
	active: pageActive,
	search: searchWithErrors,
	persistentQueryParams: [],
	maxResultsOptions: computed(() => [10, 20, 50]),
})

// Modpacks and FTB modpacks share Discover's modpack filters, so switching
// between them needs its own search.
watch(tab, (next, previous) => {
	if (!pageActive.value || next === previous) return
	const nextType = browseTypeOf(next)
	if (nextType === projectType.value) {
		searchState.currentPage.value = 1
		void searchState.refreshSearch()
	} else {
		projectType.value = nextType
	}
})

void searchState.refreshSearch()

const shownFilters = computed(() =>
	tab.value === 'mod' || tab.value === 'modpack' || tab.value === 'ftb'
		? ['game_version', 'mod_loader']
		: ['game_version'],
)
const hiddenFilterTypes = computed(() =>
	searchState.filters.value
		.map((filter) => filter.id)
		.filter((id) => !shownFilters.value.includes(id)),
)

const selectableProjectTypes = computed(() => [
	{
		label: formatMessage(messages.modpacks),
		href: '/curseforge/modpack',
		shown: cfAvailable.value,
	},
	{ label: formatMessage(messages.ftb), href: '/curseforge/ftb' },
	{ label: formatMessage(messages.mods), href: '/curseforge/mod', shown: cfAvailable.value },
	{
		label: formatMessage(messages.resourcePacks),
		href: '/curseforge/resourcepack',
		shown: cfAvailable.value,
	},
	{
		label: formatMessage(messages.dataPacks),
		href: '/curseforge/datapack',
		shown: cfAvailable.value,
	},
	{ label: formatMessage(messages.shaders), href: '/curseforge/shader', shown: cfAvailable.value },
])

const installModal = ref<InstanceType<typeof InstallToInstanceModal>>()
const manualModal = ref<InstanceType<typeof ManualDownloadsModal>>()

function onContentInstalled(result: CurseForgeModInstall, instance: GameInstance) {
	if (result.manual_downloads.length > 0) {
		manualModal.value?.show(result.manual_downloads, instance.name)
	}
}

async function startPackInstall(
	key: string,
	name: string,
	start: () => Promise<PackInstallReport>,
) {
	installing.value = new Set([...installing.value, key])
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
		const next = new Set(installing.value)
		next.delete(key)
		installing.value = next
	}
}

function getCardActions(result: Labrinth.Search.v3.ResultSearchProject): CardAction[] {
	const key = `${tab.value}-${result.project_id}`
	if (tab.value === 'ftb') {
		const pack = ftbPacks.value.get(result.project_id)
		const version =
			pack?.versions.find((other) => other.release_type === 'release') ?? pack?.versions[0]
		if (!pack || !version) return []
		return [
			{
				key: 'install',
				label: formatMessage(messages.install),
				icon: DownloadIcon,
				color: 'brand',
				disabled: installing.value.has(key),
				onClick: () =>
					startPackInstall(key, pack.name, () => ftb_install_pack(pack.id, version.id)),
			},
		]
	}
	const project = cfProjects.value.get(result.project_id)
	if (!project) return []
	if (!project.allow_distribution) {
		return [
			{
				key: 'manual',
				label: formatMessage(messages.downloadManually),
				icon: ExternalIcon,
				onClick: () => {
					if (project.website_url) void openUrl(project.website_url)
				},
			},
		]
	}
	return [
		{
			key: 'install',
			label: formatMessage(messages.install),
			icon: DownloadIcon,
			color: 'brand',
			disabled: installing.value.has(key),
			onClick: () => {
				if (tab.value === 'modpack') {
					void startPackInstall(key, project.name, () => curseforge_install_modpack(project.id))
				} else {
					installModal.value?.show(project)
				}
			},
		},
	]
}

provideBrowseManager({
	tags,
	projectType,
	...searchState,
	getProjectLink: (result: Hit) => `/curseforge/${tab.value}/${result.project_id}`,
	getServerProjectLink: (result: Hit) => `/curseforge/${tab.value}/${result.project_id}`,
	getAuthorLink: (result: Hit) =>
		tab.value === 'ftb'
			? ftbPacks.value.get(result.project_id)?.website_url
			: (cfProjects.value.get(result.project_id)?.website_url ?? undefined),
	selectableProjectTypes,
	showProjectTypeTabs: computed(() => true),
	variant: 'app',
	getCardActions,
	hiddenFilterTypes,
	maxResultsOptions: computed(() => [10, 20, 50]),
})
</script>

<template>
	<div class="flex flex-col gap-2 p-6">
		<Admonition
			v-if="!cfAvailable"
			type="info"
			:header="formatMessage(messages.notSetUp)"
			class="mb-2"
		>
			{{ formatMessage(messages.notSetUpDescription) }}
		</Admonition>
		<BrowsePageLayout />
		<InstallToInstanceModal ref="installModal" @installed="onContentInstalled" />
		<ManualDownloadsModal ref="manualModal" />
		<Teleport v-if="pageActive" to="#sidebar-teleport-target">
			<BrowseSidebar />
		</Teleport>
	</div>
</template>
