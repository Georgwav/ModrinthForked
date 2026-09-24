import { reactive, ref, watch } from 'vue'

/** Threadrinth amber. */
export const DEFAULT_ACCENT_HUE = 30

const ACCENT_HUE_KEY = 'threadrinth-accent-hue'

/** Stored instead of a hue for the white accent. */
const WHITE = 'white'

/** Lightness of the accent in dark and light themes; must match `accent.scss`. */
const DARK_THEME_ACCENT = { saturation: 1, lightness: 0.62 }
const LIGHT_THEME_ACCENT = { saturation: 1, lightness: 0.44 }
/** White on dark themes; a near-black graphite on light ones, where white would vanish. */
const DARK_THEME_WHITE = { saturation: 0, lightness: 0.92 }
const LIGHT_THEME_WHITE = { saturation: 0, lightness: 0.22 }

function normalizeHue(value: number): number {
	return ((Math.round(value) % 360) + 360) % 360
}

function loadWhite(): boolean {
	try {
		return window.localStorage.getItem(ACCENT_HUE_KEY) === WHITE
	} catch {
		return false
	}
}

function loadHue(): number {
	try {
		const stored = window.localStorage.getItem(ACCENT_HUE_KEY)
		const parsed = stored === null ? NaN : Number(stored)
		if (Number.isFinite(parsed)) {
			return normalizeHue(parsed)
		}
	} catch {
		// storage blocked or full
	}

	return DEFAULT_ACCENT_HUE
}

function hslToRgb(hue: number, saturation: number, lightness: number): [number, number, number] {
	const chroma = (1 - Math.abs(2 * lightness - 1)) * saturation
	const segment = hue / 60
	const x = chroma * (1 - Math.abs((segment % 2) - 1))
	const [r, g, b] =
		segment < 1
			? [chroma, x, 0]
			: segment < 2
				? [x, chroma, 0]
				: segment < 3
					? [0, chroma, x]
					: segment < 4
						? [0, x, chroma]
						: segment < 5
							? [x, 0, chroma]
							: [chroma, 0, x]
	const offset = lightness - chroma / 2
	return [r + offset, g + offset, b + offset]
}

function relativeLuminance([r, g, b]: [number, number, number]): number {
	const linear = (channel: number) =>
		channel <= 0.03928 ? channel / 12.92 : ((channel + 0.055) / 1.055) ** 2.4
	return 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b)
}

/** Black or white, whichever reads better on the accent. */
function contrastText(hue: number, accent: { saturation: number; lightness: number }): string {
	const luminance = relativeLuminance(hslToRgb(hue, accent.saturation, accent.lightness))
	const onBlack = (luminance + 0.05) / 0.05
	const onWhite = 1.05 / (luminance + 0.05)
	return onBlack >= onWhite ? '#000000' : '#ffffff'
}

function applyAccent(hue: number, white: boolean) {
	const style = document.documentElement.style
	const dark = white ? DARK_THEME_WHITE : DARK_THEME_ACCENT
	const light = white ? LIGHT_THEME_WHITE : LIGHT_THEME_ACCENT
	style.setProperty('--th-accent-hue', String(hue))
	style.setProperty('--th-accent-sat', String(dark.saturation))
	style.setProperty('--th-accent-l-dark', `${dark.lightness * 100}%`)
	style.setProperty('--th-accent-l-light', `${light.lightness * 100}%`)
	style.setProperty('--th-accent-contrast-dark', contrastText(hue, dark))
	style.setProperty('--th-accent-contrast-light', contrastText(hue, light))
}

const hue = ref(loadHue())
const white = ref(loadWhite())

watch(
	[hue, white],
	([hueValue, whiteValue]) => {
		applyAccent(hueValue, whiteValue)
		try {
			if (whiteValue) {
				window.localStorage.setItem(ACCENT_HUE_KEY, WHITE)
			} else if (hueValue === DEFAULT_ACCENT_HUE) {
				window.localStorage.removeItem(ACCENT_HUE_KEY)
			} else {
				window.localStorage.setItem(ACCENT_HUE_KEY, String(hueValue))
			}
		} catch {
			// storage blocked or full
		}
	},
	{ immediate: true },
)

const accentColor = reactive({
	hue,
	white,
	set(value: number) {
		white.value = false
		hue.value = normalizeHue(value)
	},
	setWhite() {
		white.value = true
	},
	reset() {
		white.value = false
		hue.value = DEFAULT_ACCENT_HUE
	},
})

export function useAccentColor() {
	return accentColor
}
