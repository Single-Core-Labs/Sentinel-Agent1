export interface ThemeColors {
  background: string;
  foreground: string;
  accent: string;
  muted: string;
  success: string;
  error: string;
  warning: string;
  info: string;
  border: string;
  cardBg: string;
  toolCallBg: string;
  planBg: string;
  inputBg: string;
  statusBarBg: string;
  selectionBg: string;
  link: string;
  spinner: string;
}

export interface ThemeConfig {
  name: string;
  colors: ThemeColors;
  spinnerFrames: string[];
}

const DARK: ThemeConfig = {
  name: 'dark',
  colors: {
    background: '#0B0D10',
    foreground: '#E6EEF8',
    accent: '#FF9D00',
    muted: '#98A0AA',
    success: '#2FCC71',
    error: '#E05A4F',
    warning: '#FF9D00',
    info: '#58A6FF',
    border: '#30363D',
    cardBg: '#121416',
    toolCallBg: '#1A1D22',
    planBg: '#0F1316',
    inputBg: '#121416',
    statusBarBg: '#0F1316',
    selectionBg: '#FF9D00',
    link: '#58A6FF',
    spinner: '#FF9D00',
  },
  spinnerFrames: ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'],
};

const HIGH_CONTRAST: ThemeConfig = {
  name: 'high-contrast',
  colors: {
    background: '#000000',
    foreground: '#FFFFFF',
    accent: '#FFFF00',
    muted: '#CCCCCC',
    success: '#00FF00',
    error: '#FF0000',
    warning: '#FFFF00',
    info: '#00FFFF',
    border: '#888888',
    cardBg: '#111111',
    toolCallBg: '#1A1A1A',
    planBg: '#0A0A0A',
    inputBg: '#111111',
    statusBarBg: '#000000',
    selectionBg: '#FFFF00',
    link: '#00FFFF',
    spinner: '#FFFF00',
  },
  spinnerFrames: ['|', '/', '-', '\\'],
};

export const THEMES: Record<string, ThemeConfig> = {
  dark: DARK,
  'high-contrast': HIGH_CONTRAST,
};
