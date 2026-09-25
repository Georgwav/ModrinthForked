<script setup lang="ts">
import { CopyIcon, MoveIcon, XIcon } from '@modrinth/assets'
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
	Toggle,
	useVIntl,
} from '@modrinth/ui'
import { computed, ref } from 'vue'

import { getInstanceIconUrl, list } from '@/helpers/instance'
import { transfer_world } from '@/helpers/threadrinth'
import type { GameInstance } from '@/helpers/types'
import { getWorldDisplayName, type SingleplayerWorld } from '@/helpers/worlds.ts'

const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()

const props = defineProps<{
	instance: GameInstance
}>()

const emit = defineEmits<{
	transferred: [world: SingleplayerWorld, moved: boolean]
}>()

const messages = defineMessages({
	title: { id: 'app.instance.worlds.transfer.title', defaultMessage: 'Copy to instance' },
	moveTitle: { id: 'app.instance.worlds.transfer.move-title', defaultMessage: 'Move to instance' },
	targetLabel: { id: 'app.instance.worlds.transfer.target', defaultMessage: 'Instance' },
	targetPlaceholder: {
		id: 'app.instance.worlds.transfer.target-placeholder',
		defaultMessage: 'Choose an instance',
	},
	moveLabel: { id: 'app.instance.worlds.transfer.move', defaultMessage: 'Move instead' },
	moveDescription: {
		id: 'app.instance.worlds.transfer.move-description',
		defaultMessage: 'Removes the world from this instance after copying it.',
	},
	olderVersionHeader: {
		id: 'app.instance.worlds.transfer.version-header',
		defaultMessage: 'Different Minecraft version',
	},
	olderVersionBody: {
		id: 'app.instance.worlds.transfer.version-body',
		defaultMessage:
			'{target} uses Minecraft {targetVersion}, this instance uses {sourceVersion}. Opening a world in an older version than it was last played in can break it.',
	},
	copyButton: { id: 'app.instance.worlds.transfer.copy-button', defaultMessage: 'Copy world' },
	moveButton: { id: 'app.instance.worlds.transfer.move-button', defaultMessage: 'Move world' },
	copied: { id: 'app.instance.worlds.transfer.copied', defaultMessage: 'World copied' },
	moved: { id: 'app.instance.worlds.transfer.moved', defaultMessage: 'World moved' },
	doneDescription: {
		id: 'app.instance.worlds.transfer.done-description',
		defaultMessage: '{world} is now in {target}.',
	},
})

const modal = ref<InstanceType<typeof NewModal>>()
const world = ref<SingleplayerWorld | null>(null)
const instances = ref<GameInstance[]>([])
const targetId = ref<string>()
const move = ref(false)
const transferring = ref(false)

const iconsById = computed(
	() => new Map(instances.value.map((other) => [other.id, getInstanceIconUrl(other.icon_path)])),
)
const targetOptions = computed<ComboboxOption<string>[]>(() =>
	instances.value
		.filter((other) => other.id !== props.instance.id && !other.quarantined)
		.sort((a, b) => a.name.localeCompare(b.name))
		.map((other) => ({
			value: other.id,
			label: other.name,
			subLabel: `${other.loader} ${other.game_version}`,
		})),
)
const target = computed(() => instances.value.find((other) => other.id === targetId.value))
const versionDiffers = computed(
	() => !!target.value && target.value.game_version !== props.instance.game_version,
)

async function show(selected: SingleplayerWorld, mode: 'copy' | 'move' = 'copy') {
	world.value = selected
	targetId.value = undefined
	move.value = mode === 'move'
	modal.value?.show()
	instances.value = await list().catch((error) => {
		handleError(error)
		return []
	})
}

function hide() {
	modal.value?.hide()
}

async function transfer() {
	if (!world.value || !target.value) return
	const selected = world.value
	const destination = target.value
	const moved = move.value
	transferring.value = true
	try {
		await transfer_world(props.instance.id, selected.path, destination.id, moved ? 'move' : 'copy')
		hide()
		addNotification({
			title: formatMessage(moved ? messages.moved : messages.copied),
			text: formatMessage(messages.doneDescription, {
				world: getWorldDisplayName(selected),
				target: destination.name,
			}),
			type: 'success',
		})
		emit('transferred', selected, moved)
	} catch (error) {
		handleError(error as Error)
	} finally {
		transferring.value = false
	}
}

defineExpose({ show, hide })
</script>

<template>
	<NewModal
		ref="modal"
		:header="formatMessage(move ? messages.moveTitle : messages.title)"
		max-width="520px"
	>
		<div class="flex flex-col gap-4">
			<p v-if="world" class="m-0 font-semibold text-contrast">
				{{ getWorldDisplayName(world) }}
			</p>
			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">{{ formatMessage(messages.targetLabel) }}</span>
				<Combobox
					v-model="targetId"
					:options="targetOptions"
					:placeholder="formatMessage(messages.targetPlaceholder)"
					searchable
					sync-with-selection
				>
					<template #option="{ item, isSelected }">
						<div class="flex items-center gap-3">
							<Avatar
								:src="iconsById.get(item.value)"
								size="36px"
								no-shadow
								class="!rounded-lg shrink-0"
							/>
							<div class="flex flex-col gap-1">
								<span
									class="font-semibold leading-tight"
									:class="isSelected ? 'text-green' : 'text-primary'"
								>
									{{ item.label }}
								</span>
								<span class="text-sm text-secondary">{{ item.subLabel }}</span>
							</div>
						</div>
					</template>
				</Combobox>
			</label>
			<div class="flex items-center justify-between gap-4">
				<div class="flex flex-col">
					<span class="font-semibold text-contrast">{{ formatMessage(messages.moveLabel) }}</span>
					<span class="text-sm text-secondary">{{ formatMessage(messages.moveDescription) }}</span>
				</div>
				<Toggle id="transfer-world-move" v-model="move" />
			</div>
			<Admonition
				v-if="versionDiffers && target"
				type="warning"
				:header="formatMessage(messages.olderVersionHeader)"
			>
				{{
					formatMessage(messages.olderVersionBody, {
						target: target.name,
						targetVersion: target.game_version,
						sourceVersion: instance.game_version,
					})
				}}
			</Admonition>
		</div>

		<template #actions>
			<div class="flex flex-wrap justify-end gap-2">
				<Button type="outlined" @click="hide">
					<XIcon />
					{{ formatMessage(commonMessages.cancelButton) }}
				</Button>
				<Button type="colored" color="brand" :disabled="!target || transferring" @click="transfer">
					<MoveIcon v-if="move" aria-hidden="true" />
					<CopyIcon v-else aria-hidden="true" />
					{{ formatMessage(move ? messages.moveButton : messages.copyButton) }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>
