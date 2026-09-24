<script setup lang="ts">
import { PlusIcon, RefreshCwIcon, SearchIcon, SquarePlusIcon } from '@modrinth/assets'
import { Button, defineMessages, injectNotificationManager, Input, useVIntl } from '@modrinth/ui'
import { useQueryClient } from '@tanstack/vue-query'
import { computed, inject, ref } from 'vue'

import FilterMenu from '@/components/ui/library/library-toolbar/filter-menu.vue'
import NewGroupModal from '@/components/ui/library/library-toolbar/new-group-modal.vue'
import SortMenu from '@/components/ui/library/library-toolbar/sort-menu.vue'
import { useLibrary } from '@/components/ui/library/use-library'
import { toError } from '@/helpers/errors'
import { refresh as refreshInstances } from '@/helpers/instance'
import { instanceKeys } from '@/pages/instance/query-options'

const { search, selectedLibraryInstances, openNewGroupModal } = useLibrary()
const showCreationModal = inject<() => void>('showCreationModal')
const { formatMessage } = useVIntl()
const { addNotification, handleError } = injectNotificationManager()
const queryClient = useQueryClient()
const refreshing = ref(false)
const messages = defineMessages({
	search: { id: 'app.library.search.placeholder', defaultMessage: 'Search' },
	newGroup: { id: 'app.library.group.new', defaultMessage: 'New group' },
	newInstance: { id: 'app.library.instance.new', defaultMessage: 'New instance' },
	refresh: { id: 'app.library.refresh', defaultMessage: 'Refresh' },
	refreshTooltip: {
		id: 'app.library.refresh.tooltip',
		defaultMessage: 'Rescan the instances folder for new or changed instances',
	},
	refreshFound: {
		id: 'app.library.refresh.found',
		defaultMessage:
			'{count, plural, one {Found # instance} other {Found # instances}} in the instances folder',
	},
	refreshSkipped: {
		id: 'app.library.refresh.skipped',
		defaultMessage:
			'{count, plural, one {# folder could not be imported} other {# folders could not be imported}}',
	},
	refreshSkippedDetail: {
		id: 'app.library.refresh.skipped.detail',
		defaultMessage: '{folders}. Check the launcher logs for details.',
	},
})
const selectedInstanceIds = computed(
	() =>
		new Set([...selectedLibraryInstances.value.values()].map((selection) => selection.instanceId)),
)

function openNewGroup() {
	openNewGroupModal(selectedInstanceIds.value)
}

async function refresh() {
	if (refreshing.value) return
	refreshing.value = true
	try {
		const report = await refreshInstances()
		await queryClient.invalidateQueries({ queryKey: instanceKeys.all })

		const found = report.imported.length + report.relocated.length
		if (found > 0) {
			addNotification({
				type: 'success',
				title: formatMessage(messages.refreshFound, { count: found }),
			})
		}
		if (report.skipped.length > 0) {
			addNotification({
				type: 'warning',
				title: formatMessage(messages.refreshSkipped, { count: report.skipped.length }),
				text: formatMessage(messages.refreshSkippedDetail, {
					folders: report.skipped.map(([folder]) => folder).join(', '),
				}),
			})
		}
	} catch (error) {
		handleError(toError(error))
	} finally {
		refreshing.value = false
	}
}
</script>

<template>
	<div class="flex flex-col gap-2">
		<div class="flex flex-wrap gap-2">
			<Input
				v-model="search"
				:icon="SearchIcon"
				type="text"
				:placeholder="formatMessage(messages.search)"
				clearable
				wrapper-class="min-w-[16rem] flex-1"
			/>
			<Button
				v-tooltip="formatMessage(messages.refreshTooltip)"
				:disabled="refreshing"
				@click="refresh"
			>
				<RefreshCwIcon :class="{ 'animate-spin': refreshing }" />
				{{ formatMessage(messages.refresh) }}
			</Button>
			<Button @click="openNewGroup">
				<SquarePlusIcon />
				{{ formatMessage(messages.newGroup) }}
			</Button>
			<Button type="colored" color="brand" @click="showCreationModal?.()">
				<PlusIcon />
				{{ formatMessage(messages.newInstance) }}
			</Button>
		</div>
		<div class="flex flex-wrap items-center gap-2">
			<SortMenu />
			<div class="mx-2 h-6 w-px bg-surface-5" />
			<FilterMenu />
		</div>
	</div>
	<NewGroupModal />
</template>
