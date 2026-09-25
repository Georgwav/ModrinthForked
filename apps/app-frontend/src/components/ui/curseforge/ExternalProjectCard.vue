<script setup lang="ts">
import { ChevronDownIcon, ExternalIcon } from '@modrinth/assets'
import { Avatar, Button, defineMessages, TagItem, useVIntl } from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { computed } from 'vue'

const { formatMessage } = useVIntl()

const props = defineProps<{
	name: string
	summary: string
	iconUrl: string | null
	authors: string[]
	stats: string[]
	tags: string[]
	websiteUrl: string | null
	expanded: boolean
}>()

// The same tag can come from several sources (like a pack's newest version
// and its own tags).
const uniqueTags = computed(() => [
	...new Map(props.tags.map((tag) => [tag.toLowerCase(), tag])).values(),
])

const emit = defineEmits<{
	toggle: []
}>()

const messages = defineMessages({
	byAuthors: { id: 'app.curseforge.card.by-authors', defaultMessage: 'by {authors}' },
	versions: { id: 'app.curseforge.card.versions', defaultMessage: 'Versions' },
	openWebsite: { id: 'app.curseforge.card.open-website', defaultMessage: 'Open website' },
})

function openWebsite() {
	if (props.websiteUrl) void openUrl(props.websiteUrl)
}
</script>

<template>
	<div class="flex flex-col gap-3 rounded-2xl bg-bg-raised p-4">
		<div class="flex gap-4">
			<Avatar :src="iconUrl" :alt="name" size="5rem" />
			<div class="flex min-w-0 flex-1 flex-col gap-1">
				<div class="flex flex-wrap items-baseline gap-x-2">
					<span class="text-lg font-bold text-contrast">{{ name }}</span>
					<span v-if="authors.length" class="text-sm text-secondary">
						{{ formatMessage(messages.byAuthors, { authors: authors.join(', ') }) }}
					</span>
				</div>
				<p class="m-0 line-clamp-2 text-secondary">{{ summary }}</p>
				<div class="mt-1 flex flex-wrap items-center gap-2">
					<span v-for="stat in stats" :key="stat" class="text-sm font-semibold text-secondary">
						{{ stat }}
					</span>
					<TagItem v-for="tag in uniqueTags" :key="tag">{{ tag }}</TagItem>
				</div>
			</div>
			<div class="flex shrink-0 flex-col items-end gap-2">
				<slot name="actions" />
				<div class="flex gap-2">
					<Button v-if="websiteUrl" type="quiet" size="sm" @click="openWebsite">
						<ExternalIcon />
						{{ formatMessage(messages.openWebsite) }}
					</Button>
					<Button type="quiet" size="sm" @click="emit('toggle')">
						<ChevronDownIcon :class="{ 'rotate-180': expanded }" class="transition-transform" />
						{{ formatMessage(messages.versions) }}
					</Button>
				</div>
			</div>
		</div>
		<div v-if="expanded" class="border-0 border-t border-solid border-divider pt-3">
			<slot />
		</div>
	</div>
</template>
