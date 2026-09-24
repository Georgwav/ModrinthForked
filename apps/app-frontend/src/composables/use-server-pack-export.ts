import { FolderOpenIcon } from '@modrinth/assets'
import {
	commonMessages,
	defineMessages,
	injectNotificationManager,
	injectPopupNotificationManager,
	useVIntl,
} from '@modrinth/ui'
import { save } from '@tauri-apps/plugin-dialog'

import { export_server_pack } from '@/helpers/threadrinth'
import type { GameInstance } from '@/helpers/types'
import { highlightInFolder } from '@/helpers/utils'

const messages = defineMessages({
	exporting: {
		id: 'app.server-pack.exporting',
		defaultMessage: 'Exporting server pack',
	},
	exportingDescription: {
		id: 'app.server-pack.exporting-description',
		defaultMessage: 'Collecting the mods and downloading the server launcher for {name}.',
	},
	exported: {
		id: 'app.server-pack.exported',
		defaultMessage: 'Server pack exported',
	},
	exportedDescription: {
		id: 'app.server-pack.exported-description',
		defaultMessage:
			'{count, plural, one {# mod} other {# mods}} included{clientOnly, plural, =0 {} one {, # client-only mod left out} other {, # client-only mods left out}}. Run start.sh or start.bat to start the server; README.txt explains the rest.',
	},
	zipFiles: {
		id: 'app.server-pack.zip-files',
		defaultMessage: 'Zip archive',
	},
})

/** Exports an instance as a ready-to-run server zip, asking where to save it. */
export function useServerPackExport() {
	const { formatMessage } = useVIntl()
	const { addNotification, handleError } = injectNotificationManager()
	const popupNotificationManager = injectPopupNotificationManager()

	return async function exportServerPack(instance: GameInstance) {
		const outputPath = await save({
			defaultPath: `${instance.name} server.zip`,
			filters: [{ name: formatMessage(messages.zipFiles), extensions: ['zip'] }],
		})
		if (!outputPath) return

		addNotification({
			title: formatMessage(messages.exporting),
			text: formatMessage(messages.exportingDescription, { name: instance.name }),
			type: 'info',
		})
		try {
			const report = await export_server_pack(instance.id, outputPath)
			popupNotificationManager.addPopupNotification({
				title: formatMessage(messages.exported),
				text: formatMessage(messages.exportedDescription, {
					count: report.mods_included,
					clientOnly: report.client_only_mods.length,
				}),
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
}
