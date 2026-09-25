<script setup lang="ts">
import { ExternalIcon } from '@modrinth/assets'
import { Button, defineMessages, NewModal, useVIntl } from '@modrinth/ui'
import { openUrl } from '@tauri-apps/plugin-opener'
import { ref } from 'vue'

import type { ManualDownload } from '@/helpers/curseforge'

const { formatMessage } = useVIntl()

const messages = defineMessages({
	header: { id: 'app.curseforge.manual.header', defaultMessage: 'Download these by hand' },
	description: {
		id: 'app.curseforge.manual.description',
		defaultMessage:
			'The authors of these files don’t allow downloading them in other apps. Download each one from its page and put it in the named folder of {instance}.',
	},
	folder: { id: 'app.curseforge.manual.folder', defaultMessage: 'into {folder}' },
	download: { id: 'app.curseforge.manual.download', defaultMessage: 'Download manually' },
	done: { id: 'app.curseforge.manual.done', defaultMessage: 'Done' },
})

const modal = ref<InstanceType<typeof NewModal>>()
const files = ref<ManualDownload[]>([])
const instanceName = ref('')

function show(downloads: ManualDownload[], instance: string) {
	files.value = downloads
	instanceName.value = instance
	modal.value?.show()
}

defineExpose({ show })
</script>

<template>
	<NewModal ref="modal" :header="formatMessage(messages.header)" scrollable max-width="640px">
		<div class="flex flex-col gap-4">
			<p class="m-0 text-secondary">
				{{ formatMessage(messages.description, { instance: instanceName }) }}
			</p>
			<ul class="m-0 flex list-none flex-col gap-2 p-0">
				<li
					v-for="file in files"
					:key="`${file.folder}/${file.name}/${file.url}`"
					class="flex items-center justify-between gap-4 rounded-xl bg-bg p-3"
				>
					<div class="flex min-w-0 flex-col">
						<span class="truncate font-semibold text-contrast">{{ file.name }}</span>
						<span v-if="file.folder" class="text-sm text-secondary">
							{{ formatMessage(messages.folder, { folder: file.folder }) }}
						</span>
					</div>
					<Button size="sm" @click="openUrl(file.url)">
						<ExternalIcon />
						{{ formatMessage(messages.download) }}
					</Button>
				</li>
			</ul>
		</div>
		<template #actions>
			<div class="flex justify-end">
				<Button type="colored" color="brand" @click="modal?.hide()">
					{{ formatMessage(messages.done) }}
				</Button>
			</div>
		</template>
	</NewModal>
</template>
