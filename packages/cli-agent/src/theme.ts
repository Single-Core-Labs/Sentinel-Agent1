import { createMemo, createSignal } from 'solid-js'

export type ThemeName =
  | 'ainight'
  | 'aiday'
  | 'tokyonight'
  | 'rosepine-moon'
  | 'oscura-midnight'
  | 'auto'

export interface Palette {
  bg: string
  surface: string
  sep: string
  accent: string
  green: string
  red: string
  yellow: string
  dim: string
  fg: string
}

// Palettes ported from the ai TUI (sentinel-ai-pager-render themes):
// ainight/aiday = neutral ramp + TokyoNight accents; tokyonight,
// rosepine-moon, oscura-midnight are the stock palettes.
export const THEMES: Record<Exclude<ThemeName, 'auto'>, Palette> = {
  ainight: {
    bg: '#141414',
    surface: '#242424',
    sep: '#1c1c1c',
    accent: '#bb9af7',
    green: '#9ece6a',
    red: '#f7768e',
    yellow: '#e0af68',
    dim: '#6c6c6c',
    fg: '#e1e1e1',
  },
  aiday: {
    bg: '#eeeeee',
    surface: '#dedede',
    sep: '#eaeaea',
    accent: '#7d4bc6',
    green: '#378e23',
    red: '#cd3048',
    yellow: '#a27612',
    dim: '#767676',
    fg: '#262626',
  },
  tokyonight: {
    bg: '#1a1b26',
    surface: '#292e42',
    sep: '#1f2335',
    accent: '#bb9af7',
    green: '#9ece6a',
    red: '#f7768e',
    yellow: '#e0af68',
    dim: '#565f89',
    fg: '#c0caf5',
  },
  'rosepine-moon': {
    bg: '#232136',
    surface: '#2a273f',
    sep: '#292642',
    accent: '#c4a7e7',
    green: '#9ccfd8',
    red: '#eb6f92',
    yellow: '#f6c177',
    dim: '#6e6a86',
    fg: '#e0def4',
  },
  'oscura-midnight': {
    bg: '#030304',
    surface: '#0f1216',
    sep: '#040406',
    accent: '#c4a7e7',
    green: '#50b48c',
    red: '#dc5a64',
    yellow: '#ebd96e',
    dim: '#81868f',
    fg: '#e4e4e4',
  },
}

const VALID = new Set<ThemeName>([...(Object.keys(THEMES) as ThemeName[]), 'auto'])

export const VALID_THEMES: ReadonlySet<string> = VALID

export function initialTheme(): ThemeName {
  const fromEnv = (Bun.env.SENTINEL_THEME as string | undefined)?.trim().toLowerCase()
  return fromEnv && VALID.has(fromEnv as ThemeName) ? (fromEnv as ThemeName) : 'auto'
}

export const themeName = createSignal<ThemeName>(initialTheme())
export const getThemeName = themeName[0]
export const setThemeName = themeName[1]

/** Resolved palette: `auto` falls back to ainight (dark default). */
export const theme = createMemo<Palette>(() => {
  const name = getThemeName()
  return THEMES[name === 'auto' ? 'ainight' : name]
})

export function themeNames(): string[] {
  return [...Object.keys(THEMES), 'auto']
}
