import { prepareThemeColorTransition } from '@modrinth/ui'
import { computed, reactive, ref, watch } from 'vue'

export const THEME_OPTIONS = [
	'dark',
	'light',
	'oled',
	'ember',
	'sand',
	'orchid',
	'amethyst',
	'blossom',
	'retro',
	'system',
] as const
export const DARK_THEMES = ['dark', 'oled', 'ember', 'orchid', 'amethyst', 'retro'] as const

export type ColorTheme = (typeof THEME_OPTIONS)[number]
export type DarkTheme = (typeof DARK_THEMES)[number]
type Theme = Exclude<ColorTheme, 'system'>
type NativeTheme = 'light' | 'dark'

const PREFERRED_THEME_KEY = 'modrinth-theme'
const PREFERRED_DARK_THEME_KEY = 'modrinth-preferred-dark-theme'

export function isDarkTheme(theme: string): theme is DarkTheme {
	return (DARK_THEMES as readonly string[]).includes(theme)
}

/** Threadrinth's themes (`themes.scss`) are variants of Modrinth's dark or light theme. */
const THEME_BASES = {
	ember: 'dark',
	sand: 'light',
	orchid: 'dark',
	amethyst: 'dark',
	blossom: 'light',
} as const

type ThreadrinthTheme = keyof typeof THEME_BASES

/**
 * The Modrinth theme a theme is based on, for places outside this app that only
 * know Modrinth's themes, like the appearance synced to a Modrinth account.
 */
export function baseTheme<T extends ColorTheme>(
	theme: T,
): Exclude<T, ThreadrinthTheme> | 'dark' | 'light' {
	return theme in THEME_BASES
		? THEME_BASES[theme as ThreadrinthTheme]
		: (theme as Exclude<T, ThreadrinthTheme>)
}

function loadPreferredTheme(): ColorTheme {
	try {
		const stored = window.localStorage.getItem(PREFERRED_THEME_KEY)
		if (stored && (THEME_OPTIONS as readonly string[]).includes(stored)) {
			return stored as ColorTheme
		}
	} catch {
		// storage blocked or full
	}

	for (const option of THEME_OPTIONS) {
		if (option !== 'system' && document.documentElement.classList.contains(`${option}-mode`)) {
			return option
		}
	}

	return 'dark'
}

function loadPreferredDarkTheme(): DarkTheme {
	try {
		const stored = window.localStorage.getItem(PREFERRED_DARK_THEME_KEY)
		if (stored && isDarkTheme(stored)) {
			return stored
		}
	} catch {
		// storage blocked or full
	}

	return 'dark'
}

const preferred = ref<ColorTheme>(loadPreferredTheme())
const preview = ref<ColorTheme | null>(null)
const preferredDark = ref<DarkTheme>(loadPreferredDarkTheme())
const advancedRendering = ref(true)
const syncAcrossDevices = ref(false)
const nativeThemeQuery = window.matchMedia('(prefers-color-scheme: dark)')
const native = ref<NativeTheme>(nativeThemeQuery.matches ? 'dark' : 'light')
const active = computed<Theme>(() => {
	const selectedTheme = preview.value ?? preferred.value
	if (selectedTheme !== 'system') {
		return selectedTheme
	}

	return native.value === 'light' ? 'light' : preferredDark.value
})

nativeThemeQuery.addEventListener('change', (event) => {
	native.value = event.matches ? 'dark' : 'light'
})

watch([preferred, preview], ([selectedPreferred, selectedPreview]) => {
	const selectedTheme = selectedPreview ?? selectedPreferred
	if (isDarkTheme(selectedTheme)) {
		preferredDark.value = selectedTheme
	}
})

watch(
	preferred,
	(theme) => {
		try {
			window.localStorage.setItem(PREFERRED_THEME_KEY, theme)
		} catch {
			// storage blocked or full
		}
	},
	{ immediate: true },
)

watch(preferredDark, (theme) => {
	try {
		window.localStorage.setItem(PREFERRED_DARK_THEME_KEY, theme)
	} catch {
		// storage blocked or full
	}
})

watch(
	active,
	(theme, previousTheme) => {
		if (previousTheme && previousTheme !== theme) {
			prepareThemeColorTransition()
		}

		const html = document.documentElement
		for (const option of THEME_OPTIONS) {
			html.classList.remove(`${option}-mode`)
		}
		html.classList.add(`${baseTheme(theme)}-mode`, `${theme}-mode`)
	},
	{ immediate: true },
)

function applyAccountAppearance(appearance: { auto: boolean; theme: string }): void {
	if (isDarkTheme(appearance.theme)) {
		preferredDark.value = appearance.theme
	}

	if (appearance.auto) {
		preferred.value = 'system'
		return
	}

	if ((THEME_OPTIONS as readonly string[]).includes(appearance.theme)) {
		preferred.value = appearance.theme as ColorTheme
	}
}

const theme = reactive({
	preferred,
	preview,
	preferredDark,
	active,
	native,
	syncAcrossDevices,
	advancedRendering,
	options: THEME_OPTIONS,
	applyAccountAppearance,
})

export function useTheme() {
	return theme
}
