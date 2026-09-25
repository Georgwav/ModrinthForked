<script setup lang="ts">
import {
	CalendarIcon,
	CurseForgeIcon,
	DownloadIcon,
	ExternalIcon,
	HeartIcon,
	SpinnerIcon,
} from '@modrinth/assets'
import {
	Avatar,
	Button,
	Chips,
	defineMessages,
	injectNotificationManager,
	PageHeader,
	PageHeaderActions,
	PageHeaderMetadata,
	PageHeaderMetadataNumberItem,
	PageHeaderMetadataTagsItem,
	PageHeaderMetadataTimeItem,
	ProjectPageDescription,
	TagItem,
	useVIntl,
} from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed, ref, watch } from 'vue'
import { useRoute } from 'vue-router'

import CurseForgeVersions from '@/components/ui/curseforge/CurseForgeVersions.vue'
import InstallToInstanceModal from '@/components/ui/curseforge/InstallToInstanceModal.vue'
import ManualDownloadsModal from '@/components/ui/curseforge/ManualDownloadsModal.vue'
import {
	curseforge_description,
	curseforge_install_modpack,
	curseforge_project,
	type CurseForgeFile,
	type CurseForgeModInstall,
	type CurseForgeProject,
	ftb_install_pack,
	ftb_pack,
	type FtbPack,
	type FtbVersion,
	type PackInstallReport,
} from '@/helpers/curseforge'
import type { GameInstance } from '@/helpers/types'
import { useBreadcrumb, useRootBreadcrumb } from '@/providers/breadcrumbs'

/** A CurseForge project or Feed the Beast pack, laid out like a Modrinth project page. */

defineOptions({ name: 'CurseForgeProjectPage' })

type Tab = 'description' | 'versions'

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()
const route = useRoute()

const messages = defineMessages({
	heading: { id: 'app.curseforge.heading', defaultMessage: 'CurseForge' },
	description: { id: 'app.curseforge.project.description', defaultMessage: 'Description' },
	versions: { id: 'app.curseforge.project.versions', defaultMessage: 'Versions' },
	install: { id: 'app.curseforge.install', defaultMessage: 'Install' },
	downloadManually: {
		id: 'app.curseforge.download-manually',
		defaultMessage: 'Download manually',
	},
	openWebsite: { id: 'app.curseforge.project.open-website', defaultMessage: 'Open website' },
	downloads: {
		id: 'app.curseforge.project.downloads',
		defaultMessage: '{count, plural, one {download} other {downloads}}',
	},
	installs: {
		id: 'app.curseforge.project.installs',
		defaultMessage: '{count, plural, one {install} other {installs}}',
	},
	likes: {
		id: 'app.curseforge.project.likes',
		defaultMessage: '{count, plural, one {like} other {likes}}',
	},
	updated: { id: 'app.curseforge.project.updated', defaultMessage: 'Updated' },
	by: { id: 'app.curseforge.project.by', defaultMessage: 'By {authors}' },
	compatibility: { id: 'app.curseforge.project.compatibility', defaultMessage: 'Compatibility' },
	minecraft: { id: 'app.curseforge.project.minecraft', defaultMessage: 'Minecraft' },
	loaders: { id: 'app.curseforge.project.loaders', defaultMessage: 'Platforms' },
	creators: { id: 'app.curseforge.project.creators', defaultMessage: 'Creators' },
	versionInfo: {
		id: 'app.curseforge.ftb.version-info',
		defaultMessage: 'Minecraft {game} · {loader}',
	},
	noVersions: {
		id: 'app.curseforge.ftb.no-versions',
		defaultMessage: 'This pack has no public versions.',
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

const tabType = computed(() => String(route.params.projectType ?? 'mod'))
const projectId = computed(() => Number(route.params.id))
const isFtb = computed(() => tabType.value === 'ftb')

const cfProject = ref<CurseForgeProject | null>(null)
const ftbProject = ref<FtbPack | null>(null)
const body = ref('')
const loading = ref(true)
const installing = ref(false)
const tab = ref<Tab>('description')

const installModal = ref<InstanceType<typeof InstallToInstanceModal>>()
const manualModal = ref<InstanceType<typeof ManualDownloadsModal>>()

/** What the header and sidebar show, for either source. */
const project = computed(() => {
	if (cfProject.value) {
		const value = cfProject.value
		return {
			name: value.name,
			summary: value.summary,
			icon: value.icon_url,
			authors: value.authors,
			count: value.downloads,
			countLabel: messages.downloads,
			likes: value.thumbs_up,
			updated: value.updated,
			tags: value.categories,
			gameVersions: value.game_versions,
			loaders: value.loaders,
			website: value.website_url,
		}
	}
	if (ftbProject.value) {
		const value = ftbProject.value
		return {
			name: value.name,
			summary: value.summary,
			icon: value.icon_url,
			authors: value.authors,
			count: value.installs,
			countLabel: messages.installs,
			likes: null,
			updated: value.updated > 0 ? new Date(value.updated * 1000).toISOString() : null,
			tags: value.tags,
			gameVersions: [
				...new Set(value.versions.map((version) => version.game_version).filter(Boolean)),
			] as string[],
			loaders: [
				...new Set(value.versions.map((version) => version.loader).filter(Boolean)),
			] as string[],
			website: value.website_url,
		}
	}
	return null
})

useRootBreadcrumb({
	slot: 'root',
	id: 'curseforge',
	label: formatMessage(messages.heading),
	to: () => `/curseforge/${tabType.value}`,
	visual: { type: 'icon', component: CurseForgeIcon },
})
useBreadcrumb({
	slot: 'page',
	id: () => `curseforge-${tabType.value}-${projectId.value}`,
	label: () => project.value?.name ?? '',
	to: () => `/curseforge/${tabType.value}/${projectId.value}`,
	visual: () =>
		project.value?.icon
			? { type: 'image', src: project.value.icon, alt: project.value.name }
			: undefined,
})

async function load() {
	loading.value = true
	cfProject.value = null
	ftbProject.value = null
	body.value = ''
	try {
		if (isFtb.value) {
			ftbProject.value = await ftb_pack(projectId.value)
			body.value = ftbProject.value.description
		} else {
			const [value, description] = await Promise.all([
				curseforge_project(projectId.value),
				curseforge_description(projectId.value).catch(() => ''),
			])
			cfProject.value = value
			body.value = description || value.summary
		}
	} catch (error) {
		handleError(error as Error)
	} finally {
		loading.value = false
	}
}
watch([tabType, projectId], () => void load(), { immediate: true })


async function startPackInstall(name: string, start: () => Promise<PackInstallReport>) {
	installing.value = true
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
		installing.value = false
	}
}

function installFtb(version?: FtbVersion) {
	const pack = ftbProject.value
	const chosen =
		version ?? pack?.versions.find((other) => other.release_type === 'release') ?? pack?.versions[0]
	if (!pack || !chosen) return
	void startPackInstall(pack.name, () => ftb_install_pack(pack.id, chosen.id))
}

function installCurseForge(file?: CurseForgeFile) {
	const value = cfProject.value
	if (!value) return
	if (value.class === 'modpack') {
		void startPackInstall(value.name, () => curseforge_install_modpack(value.id, file?.id))
	} else {
		installModal.value?.show(value, file ? { id: file.id, name: file.name } : undefined)
	}
}

function install() {
	if (isFtb.value) installFtb()
	else installCurseForge()
}

function onContentInstalled(result: CurseForgeModInstall, instance: GameInstance) {
	if (result.manual_downloads.length > 0) {
		manualModal.value?.show(result.manual_downloads, instance.name)
	}
}

function ftbVersionInfo(version: FtbVersion) {
	return formatMessage(messages.versionInfo, {
		game: version.game_version ?? '?',
		loader: [version.loader, version.loader_version].filter(Boolean).join(' ') || 'vanilla',
	})
}
</script>

<template>
	<div>
		<Teleport v-if="project" to="#sidebar-teleport-target">
			<div class="flex flex-col gap-3 p-4">
				<h2 class="m-0 text-lg font-extrabold text-contrast">
					{{ formatMessage(messages.compatibility) }}
				</h2>
				<div v-if="project.gameVersions.length" class="flex flex-col gap-2">
					<span class="font-semibold text-primary">{{ formatMessage(messages.minecraft) }}</span>
					<div class="flex flex-wrap gap-1">
						<TagItem v-for="version in project.gameVersions.slice(0, 12)" :key="version">
							{{ version }}
						</TagItem>
					</div>
				</div>
				<div v-if="project.loaders.length" class="flex flex-col gap-2">
					<span class="font-semibold text-primary">{{ formatMessage(messages.loaders) }}</span>
					<div class="flex flex-wrap gap-1">
						<TagItem v-for="loader in project.loaders" :key="loader" class="capitalize">
							{{ loader }}
						</TagItem>
					</div>
				</div>
			</div>
			<div
				v-if="project.authors.length"
				class="flex flex-col gap-2 border-0 border-t border-solid border-[--brand-gradient-border] p-4"
			>
				<h2 class="m-0 text-lg font-extrabold text-contrast">
					{{ formatMessage(messages.creators) }}
				</h2>
				<span v-for="author in project.authors" :key="author" class="font-semibold">
					{{ author }}
				</span>
			</div>
		</Teleport>
		<div class="flex flex-col gap-4 p-6">
			<div v-if="loading" class="flex justify-center p-8">
				<SpinnerIcon class="size-8 animate-spin text-secondary" />
			</div>
			<template v-else-if="project">
				<PageHeader :title="project.name" :summary="project.summary">
					<template #leading>
						<Avatar :src="project.icon" :alt="project.name" size="96px" />
					</template>
					<template #metadata>
						<PageHeaderMetadata>
							<PageHeaderMetadataNumberItem
								:icon="DownloadIcon"
								:value="project.count"
								:label="formatMessage(project.countLabel, { count: project.count })"
							/>
							<PageHeaderMetadataNumberItem
								v-if="project.likes !== null"
								:icon="HeartIcon"
								:value="project.likes"
								:label="formatMessage(messages.likes, { count: project.likes })"
							/>
							<PageHeaderMetadataTimeItem
								v-if="project.updated"
								:icon="CalendarIcon"
								:date="project.updated"
								:label="formatMessage(messages.updated)"
							/>
							<PageHeaderMetadataTagsItem v-if="project.tags.length" class="hidden md:flex">
								<TagItem v-for="tag in project.tags" :key="tag">{{ tag }}</TagItem>
							</PageHeaderMetadataTagsItem>
						</PageHeaderMetadata>
					</template>
					<template #actions>
						<PageHeaderActions>
							<Button
								v-if="isFtb || cfProject?.allow_distribution"
								type="colored"
								color="brand"
								size="xl"
								:disabled="installing"
								@click="install"
							>
								<SpinnerIcon v-if="installing" class="animate-spin" />
								<DownloadIcon v-else />
								{{ formatMessage(messages.install) }}
							</Button>
							<Button
								v-if="project.website"
								size="xl"
								:type="isFtb || cfProject?.allow_distribution ? 'base' : 'colored'"
								:color="isFtb || cfProject?.allow_distribution ? undefined : 'brand'"
								@click="openUrl(project.website ?? '')"
							>
								<ExternalIcon />
								{{
									formatMessage(
										isFtb || cfProject?.allow_distribution
											? messages.openWebsite
											: messages.downloadManually,
									)
								}}
							</Button>
						</PageHeaderActions>
					</template>
				</PageHeader>

				<Chips
					v-model="tab"
					:items="['description', 'versions'] as Tab[]"
					:format-label="(item: Tab) => formatMessage(messages[item])"
					:capitalize="false"
				/>

				<div v-if="tab === 'description'" class="rounded-2xl bg-bg-raised p-6">
					<ProjectPageDescription :description="body" />
				</div>
				<template v-else>
					<div v-if="ftbProject" class="flex flex-col gap-2">
						<p v-if="ftbProject.versions.length === 0" class="m-0 text-secondary">
							{{ formatMessage(messages.noVersions) }}
						</p>
						<div
							v-for="version in ftbProject.versions"
							:key="version.id"
							class="flex items-center justify-between gap-4 rounded-2xl bg-bg-raised p-4"
						>
							<div class="flex min-w-0 flex-col gap-1">
								<span class="truncate font-semibold text-contrast">
									{{ version.name }}
									<span class="text-sm font-normal text-secondary">
										({{ version.release_type }})
									</span>
								</span>
								<span class="text-sm text-secondary">{{ ftbVersionInfo(version) }}</span>
							</div>
							<Button :disabled="installing" @click="installFtb(version)">
								<DownloadIcon />
								{{ formatMessage(messages.install) }}
							</Button>
						</div>
					</div>
					<div v-else-if="cfProject" class="rounded-2xl bg-bg-raised p-4">
						<CurseForgeVersions
							:project-id="cfProject.id"
							:game-version="null"
							:loader="null"
							:installing="installing"
							@install="installCurseForge"
						/>
					</div>
				</template>
			</template>
		</div>
		<InstallToInstanceModal ref="installModal" @installed="onContentInstalled" />
		<ManualDownloadsModal ref="manualModal" />
	</div>
</template>
