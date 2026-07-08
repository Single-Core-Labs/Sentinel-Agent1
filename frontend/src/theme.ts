export interface ThemeColors {
  foreground: string;
  muted: string;
  accent: string;
  accentAlt: string;
  success: string;
  error: string;
  warning: string;
  info: string;
  border: string;
  dimBorder: string;
  toolCallFg: string;
  planFg: string;
  userFg: string;
  assistantFg: string;
  approvalBorder: string;
  spinner: string;
}

export interface ThemeConfig {
  name: string;
  colors: ThemeColors;
  spinnerFrames: string[];
  particleChars: string[];
}

const DARK: ThemeConfig = {
  name: 'dark',
  colors: {
    foreground:    '#E2E8F0',
    muted:         '#64748B',
    accent:        '#F97316',
    accentAlt:     '#0EA5E9',
    success:       '#22C55E',
    error:         '#EF4444',
    warning:       '#F59E0B',
    info:          '#38BDF8',
    border:        '#334155',
    dimBorder:     '#1E293B',
    toolCallFg:    '#A78BFA',
    planFg:        '#34D399',
    userFg:        '#93C5FD',
    assistantFg:   '#E2E8F0',
    approvalBorder:'#F59E0B',
    spinner:       '#F97316',
  },
  spinnerFrames: ['⠋','⠙','⠹','⠸','⠼','⠴','⠦','⠧','⠇','⠏'],
  particleChars: ['·','•','◦','∘','○','◌','◎','◉','◈','◆','◇','▪','▫','▸','▹'],
};

const HIGH_CONTRAST: ThemeConfig = {
  name: 'high-contrast',
  colors: {
    foreground:    '#FFFFFF',
    muted:         '#AAAAAA',
    accent:        '#FFFF00',
    accentAlt:     '#00FFFF',
    success:       '#00FF00',
    error:         '#FF4444',
    warning:       '#FFAA00',
    info:          '#00AAFF',
    border:        '#888888',
    dimBorder:     '#444444',
    toolCallFg:    '#FF88FF',
    planFg:        '#88FFAA',
    userFg:        '#88CCFF',
    assistantFg:   '#FFFFFF',
    approvalBorder:'#FFAA00',
    spinner:       '#FFFF00',
  },
  spinnerFrames: ['|','/','-','\\'],
  particleChars: ['.','*','+','x','o','0','#','@','%','&'],
};

const CYBER: ThemeConfig = {
  name: 'cyber',
  colors: {
    foreground:    '#E0F0FF',
    muted:         '#4A6A8A',
    accent:        '#00FFD0',
    accentAlt:     '#FF006E',
    success:       '#00FF9F',
    error:         '#FF006E',
    warning:       '#FFD600',
    info:          '#00C8FF',
    border:        '#0D3B5C',
    dimBorder:     '#071E2E',
    toolCallFg:    '#FF006E',
    planFg:        '#00FFD0',
    userFg:        '#00C8FF',
    assistantFg:   '#E0F0FF',
    approvalBorder:'#FFD600',
    spinner:       '#00FFD0',
  },
  spinnerFrames: ['◐','◓','◑','◒'],
  particleChars: ['▓','▒','░','█','▄','▀','■','□','▪','▫'],
};

export const THEMES: Record<string, ThemeConfig> = {
  dark: DARK,
  'high-contrast': HIGH_CONTRAST,
  cyber: CYBER,
};
