<script setup lang="ts">
import { DownloadIcon, XIcon } from '@modrinth/assets'
import {
	Admonition,
	Avatar,
	Button,
	Combobox,
	type ComboboxOption,
	commonMessages,
	defineMessages,
	injectNotificationManager,
	NewModal,
	useVIntl,
} from '@modrinth/ui'
import { computed, ref } from 'vue'

import {
	curseforge_install_mod,
	type CurseForgeClass,
	type CurseForgeModInstall,
} from '@/helpers/curseforge'
import { getInstanceIconUrl, list } from '@/helpers/instance'
import type { GameInstance } from '@/helpers/types'

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()

const emit = defineEmits<{
	installed: [result: CurseForgeModInstall, instance: GameInstance]
}>()

const messages = defineMessages({
	title: { id: 'app.curseforge.install.title', defaultMessage: 'Install {name}' },
	instanceLabel: { id: 'app.curseforge.install.instance', defaultMessage: 'Instance' },
	instancePlaceholder: {
		id: 'app.curseforge.install.instance-placeholder',
		defaultMessage: 'Choose an instance',
	},
	newestDescription: {
		id: 'app.curseforge.install.newest-description',
		defaultMessage:
			'Installs the newest version for the instance’s Minecraft version (and mod loader, with required dependencies, for mods).',
	},
	fileDescription: {
		id: 'app.curseforge.install.file-description',
		defaultMessage: 'Installs {file} with its required dependencies.',
	},
	vanillaHeader: {
		id: 'app.curseforge.install.vanilla-header',
		defaultMessage: 'No mod loader',
	},
	vanillaBody: {
		id: 'app.curseforge.install.vanilla-body',
		defaultMessage:
			'{instance} has no mod loader. Mods need an instance with Forge, NeoForge, Fabric or Quilt.',
	},
	installButton: { id: 'app.curseforge.install.button', defaultMessage: 'Install' },
	installed: { id: 'app.curseforge.install.installed', defaultMessage: 'Installed' },
	installedDescription: {
		id: 'app.curseforge.install.installed-description',
		defaultMessage: '{count, plural, one {# file} other {# files}} added to {instance}.',
	},
	missingDependencies: {
		id: 'app.curseforge.install.missing-dependencies',
		defaultMessage: 'Some dependencies have no version for this instance',
	},
	missingDependenciesDescription: {
		id: 'app.curseforge.install.missing-dependencies-description',
		defaultMessage: 'Install these yourself if the mod needs them: {names}',
	},
})

const modal = ref<InstanceType<typeof NewModal>>()
const instances = ref<GameInstance[]>([])
const targetId = ref<string>()
const projectId = ref<number>(0)
const projectName = ref('')
const fileId = ref<number | null>(null)
const fileName = ref<string | null>(null)
const projectClass = ref<CurseForgeClass>('mod')
/** Only mods need a mod loader. */
const needsLoader = computed(
	() => projectClass.value === 'mod' && target.value?.loader === 'vanilla',
)
const instanceIcons = computed(
	() =>
		new Map(
			instances.value.map((instance) => [instance.id, getInstanceIconUrl(instance.icon_path)]),
		),
)
const installing = ref(false)

const targetOptions = computed<ComboboxOption<string>[]>(() =>
	instances.value
		.filter((instance) => !instance.quarantined)
		.sort((a, b) => a.name.localeCompare(b.name))
		.map((instance) => ({
			value: instance.id,
			label: instance.name,
			subLabel: `${instance.loader} ${instance.game_version}`,
		})),
)
const target = computed(() => instances.value.find((instance) => instance.id === targetId.value))

async function show(
	project: { id: number; name: string; class?: CurseForgeClass },
	file?: { id: number; name: string },
) {
	projectId.value = project.id
	projectClass.value = project.class ?? 'mod'
	projectName.value = project.name
	fileId.value = file?.id ?? null
	fileName.value = file?.name ?? null
	modal.value?.show()
	instances.value = await list().catch((error) => {
		handleError(error)
		return []
	})
	if (targetId.value && !instances.value.some((instance) => instance.id === targetId.value)) {
		targetId.value = undefined
	}
}

function hide() {
	modal.value?.hide()
}

async function install() {
	const instance = target.value
	if (!instance) return
	installing.value = true
	try {
		const result = await curseforge_install_mod(instance.id, projectId.value, fileId.value)
		hide()
		if (result.installed.length > 0) {
			addNotification({
				title: formatMessage(messages.installed),
				text: formatMessage(messages.installedDescription, {
					count: result.installed.length,
					instance: instance.name,
				}),
				type: 'success',
			})
		}
		if (result.missing_dependencies.length > 0) {
			addNotification({
				title: formatMessage(messages.missingDependencies),
				text: formatMessage(messages.missingDependenciesDescription, {
					names: result.missing_dependencies.join(', '),
				}),
				type: 'warning',
			})
		}
		emit('installed', result, instance)
	} catch (error) {
		handleError(error as Error)
	} finally {
		installing.value = false
	}
}

defineExpose({ show, hide })
</script>

<template>
	<NewModal
		ref="modal"
		:header="formatMessage(messages.title, { name: projectName })"
		max-width="520px"
	>
		<div class="flex flex-col gap-4">
			<p class="m-0 text-secondary">
				{{
					fileName
						? formatMessage(messages.fileDescription, { file: fileName })
						: formatMessage(messages.newestDescription)
				}}
			</p>
			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">{{ formatMessage(messages.instanceLabel) }}</span>
				<Combobox
					v-model="targetId"
					:options="targetOptions"
					:placeholder="formatMessage(messages.instancePlaceholder)"
					searchable
					sync-with-selection
				>
					<template #option="{ item, isSelected }">
						<div class="flex items-center gap-3">
							<Avatar
								:src="instanceIcons.get(item.value)"
								size="36px"
								no-shadow
								class="!rounded-lg shrink-0"
							/>
							<div class="flex min-w-0 flex-col gap-1">
								<span
									class="truncate font-semibold leading-tight"
									:class="isSelected ? 'text-green' : 'text-primary'"
								>
									{{ item.label }}
								</span>
								<span class="truncate text-sm text-secondary">{{ item.subLabel }}</span>
							</div>
						</div>
					</template>
				</Combobox>
			</label>
			<Admonition
				v-if="target && needsLoader"
				type="warning"
				:header="formatMessage(messages.vanillaHeader)"
			>
				{{ formatMessage(messages.vanillaBody, { instance: target.name }) }}
			</Admonition>
		</div>

		<template #actions>
			<div class="flex flex-wrap justify-end gap-2">
				<Button type="outlined" @click="hide">
					<XIcon />
					{{ formatMessage(commonMessages.cancelButton) }}
				</Button>
				<Button
					type="colored"
					color="brand"
					:disabled="!target || needsLoader || installing"
					:loading="installing"
					@click="install"
				>
					<DownloadIcon />
					{{ formatMessage(messages.installButton) }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>
