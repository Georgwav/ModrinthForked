<script setup lang="ts">
import { CheckIcon, UndoIcon } from '@modrinth/assets'
import { Button, defineMessages, useVIntl } from '@modrinth/ui'
import { computed } from 'vue'

import { DEFAULT_ACCENT_HUE, useAccentColor } from '@/composables/use-accent-color'

const accent = useAccentColor()
const { formatMessage } = useVIntl()

const messages = defineMessages({
	title: { id: 'app.settings.appearance.accent.title', defaultMessage: 'Accent color' },
	description: {
		id: 'app.settings.appearance.accent.description',
		defaultMessage: 'Pick the highlight color used across the app. It works with every theme.',
	},
	slider: { id: 'app.settings.appearance.accent.slider', defaultMessage: 'Accent hue' },
	reset: { id: 'app.settings.appearance.accent.reset', defaultMessage: 'Reset to amber' },
})

const presetMessages = defineMessages({
	amber: { id: 'app.settings.appearance.accent.preset.amber', defaultMessage: 'Amber' },
	red: { id: 'app.settings.appearance.accent.preset.red', defaultMessage: 'Red' },
	pink: { id: 'app.settings.appearance.accent.preset.pink', defaultMessage: 'Pink' },
	purple: { id: 'app.settings.appearance.accent.preset.purple', defaultMessage: 'Purple' },
	blue: { id: 'app.settings.appearance.accent.preset.blue', defaultMessage: 'Blue' },
	cyan: { id: 'app.settings.appearance.accent.preset.cyan', defaultMessage: 'Cyan' },
	green: { id: 'app.settings.appearance.accent.preset.green', defaultMessage: 'Green' },
	yellow: { id: 'app.settings.appearance.accent.preset.yellow', defaultMessage: 'Yellow' },
})

const presets = [
	{ hue: DEFAULT_ACCENT_HUE, name: presetMessages.amber },
	{ hue: 0, name: presetMessages.red },
	{ hue: 330, name: presetMessages.pink },
	{ hue: 270, name: presetMessages.purple },
	{ hue: 220, name: presetMessages.blue },
	{ hue: 185, name: presetMessages.cyan },
	{ hue: 145, name: presetMessages.green },
	{ hue: 55, name: presetMessages.yellow },
]

const isDefault = computed(() => accent.hue === DEFAULT_ACCENT_HUE)
</script>

<template>
	<section class="mt-8 border-0 border-t border-solid border-divider pt-6">
		<div class="flex flex-col gap-1">
			<h2 class="m-0 text-xl font-semibold text-contrast">
				{{ formatMessage(messages.title) }}
			</h2>
			<p class="m-0 text-secondary">
				{{ formatMessage(messages.description) }}
			</p>
		</div>

		<div class="mt-4 flex flex-wrap items-center gap-2">
			<button
				v-for="preset in presets"
				:key="preset.hue"
				v-tooltip="formatMessage(preset.name)"
				type="button"
				class="accent-swatch"
				:style="{ '--swatch-hue': preset.hue }"
				:aria-label="formatMessage(preset.name)"
				:aria-pressed="accent.hue === preset.hue"
				@click="accent.set(preset.hue)"
			>
				<CheckIcon v-if="accent.hue === preset.hue" aria-hidden="true" />
			</button>
		</div>

		<div class="mt-4 flex items-center gap-4">
			<input
				:value="accent.hue"
				type="range"
				min="0"
				max="359"
				step="1"
				class="accent-slider"
				:aria-label="formatMessage(messages.slider)"
				@input="accent.set(Number(($event.target as HTMLInputElement).value))"
			/>
			<Button :disabled="isDefault" @click="accent.reset()">
				<UndoIcon />
				{{ formatMessage(messages.reset) }}
			</Button>
		</div>
	</section>
</template>

<style scoped lang="scss">
.accent-swatch {
	display: grid;
	place-items: center;
	width: 2rem;
	height: 2rem;
	padding: 0;
	border: 2px solid var(--color-divider);
	border-radius: 50%;
	background: hsl(var(--swatch-hue) 100% 62%);
	color: #000000;
	cursor: pointer;
	transition: transform 0.1s ease;

	&:hover {
		transform: scale(1.08);
	}

	&[aria-pressed='true'] {
		border-color: var(--color-contrast);
	}

	svg {
		width: 1.1rem;
		height: 1.1rem;
	}
}

.accent-slider {
	flex: 1;
	min-width: 0;
	height: 0.75rem;
	padding: 0;
	border: none;
	border-radius: 999px;
	background: linear-gradient(
		to right,
		hsl(0 100% 62%),
		hsl(60 100% 62%),
		hsl(120 100% 62%),
		hsl(180 100% 62%),
		hsl(240 100% 62%),
		hsl(300 100% 62%),
		hsl(359 100% 62%)
	);
	cursor: pointer;

	&::-webkit-slider-thumb {
		appearance: none;
		width: 1.25rem;
		height: 1.25rem;
		border: 3px solid var(--color-contrast);
		border-radius: 50%;
		background: var(--color-brand);
		box-shadow: 0 0 0 2px var(--surface-1);
	}

	&::-moz-range-thumb {
		width: 1.25rem;
		height: 1.25rem;
		border: 3px solid var(--color-contrast);
		border-radius: 50%;
		background: var(--color-brand);
	}
}
</style>
