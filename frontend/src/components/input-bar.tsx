import { Box, Text, useInput } from 'ink';
import { useState } from 'react';
import type { ThemeConfig } from '../theme.js';

const SLASH_COMMANDS = [
  { command: '/model', description: 'Switch model' },
  { command: '/compact', description: 'Compact conversation context' },
  { command: '/new', description: 'Start a new session' },
  { command: '/resume', description: 'Resume last session' },
  { command: '/theme', description: 'Switch theme (dark/high-contrast)' },
  { command: '/undo', description: 'Undo last turn' },
  { command: '/help', description: 'Show available commands' },
];

interface Props {
  onSend: (text: string) => void;
  disabled?: boolean;
  theme: ThemeConfig;
}

export function InputBar({ onSend, disabled = false, theme }: Props) {
  const [value, setValue] = useState('');
  const [suggestions, setSuggestions] = useState<typeof SLASH_COMMANDS>([]);
  const [sel, setSel] = useState(0);
  const c = theme.colors;

  useInput((input, key) => {
    if (disabled) return;

    if (key.return && !key.shift && !key.ctrl) {
      const v = value.trim();
      if (!v) return;
      if (suggestions.length > 0) {
        setValue(suggestions[sel].command + ' ');
        setSuggestions([]);
        setSel(0);
        return;
      }
      onSend(v);
      setValue('');
      setSuggestions([]);
      setSel(0);
      return;
    }

    if (key.return && (key.shift || key.ctrl)) {
      setValue(v => v + '\n');
      return;
    }

    if (key.backspace) {
      setValue(v => {
        const next = v.slice(0, -1);
        updateSuggestions(next);
        return next;
      });
      return;
    }

    if (key.up && suggestions.length > 0) {
      setSel(i => Math.max(0, i - 1));
      return;
    }
    if (key.down && suggestions.length > 0) {
      setSel(i => Math.min(suggestions.length - 1, i + 1));
      return;
    }

    if (key.tab && suggestions.length > 0) {
      setValue(suggestions[sel].command + ' ');
      setSuggestions([]);
      setSel(0);
      return;
    }

    if (!input || key.ctrl || key.meta || key.escape || key.left || key.right || key.delete || key.pageDown || key.pageUp) return;

    const next = value + input;
    setValue(next);
    updateSuggestions(next);
  });

  function updateSuggestions(v: string) {
    if (v.startsWith('/') && !v.includes(' ')) {
      const m = SLASH_COMMANDS.filter(c => c.command.startsWith(v));
      setSuggestions(m);
      setSel(0);
    } else {
      setSuggestions([]);
    }
  }

  const placeholder = 'Type a message or / for commands...';

  return (
    <Box flexDirection="column">
      {suggestions.length > 0 && (
        <Box flexDirection="column" borderStyle="round" borderColor={c.border} paddingX={1}>
          {suggestions.map((cmd, i) => (
            <Box key={cmd.command}>
              <Text color={i === sel ? c.accent : c.foreground}>
                {i === sel ? '▸ ' : '  '}{cmd.command}
              </Text>
              <Text color={c.muted}> — {cmd.description}</Text>
            </Box>
          ))}
        </Box>
      )}
      <Box>
        <Text color={c.accent}>❯ </Text>
        {value ? (
          <Text color={c.foreground}>{value}</Text>
        ) : (
          <Text color={c.muted} dimColor>{placeholder}</Text>
        )}
      </Box>
    </Box>
  );
}
