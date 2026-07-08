import { Box, Text, useInput } from 'ink';
import { useState } from 'react';
import type { ThemeConfig } from '../theme.js';

// ── Slash commands ─────────────────────────────────────────────────

const SLASH_COMMANDS = [
  { command: '/model',   description: 'Switch model' },
  { command: '/theme',   description: 'Switch theme (dark | high-contrast | cyber)' },
  { command: '/compact', description: 'Compact conversation context' },
  { command: '/new',     description: 'Start a new session' },
  { command: '/resume',  description: 'Resume last session' },
  { command: '/undo',    description: 'Undo last turn' },
  { command: '/help',    description: 'Show available commands' },
  { command: '/quit',    description: 'Exit' },
];

interface Props {
  onSend: (text: string) => void;
  disabled?: boolean;
  theme: ThemeConfig;
  mode: string;
}

export function InputBar({ onSend, disabled = false, theme }: Props) {
  const [value, setValue] = useState('');
  const [suggestions, setSuggestions] = useState<typeof SLASH_COMMANDS>([]);
  const [selIdx, setSelIdx] = useState(0);
  const c = theme.colors;

  function updateSuggestions(v: string) {
    if (v.startsWith('/') && !v.includes(' ')) {
      const matches = SLASH_COMMANDS.filter(s => s.command.startsWith(v));
      setSuggestions(matches);
      setSelIdx(0);
    } else {
      setSuggestions([]);
    }
  }

  useInput((input, key) => {
    if (disabled) return;

    // Enter → send (unless shift/ctrl for newline)
    if (key.return && !key.shift && !key.ctrl) {
      if (suggestions.length > 0) {
        setValue(suggestions[selIdx]!.command + ' ');
        setSuggestions([]);
        setSelIdx(0);
        return;
      }
      const trimmed = value.trim();
      if (!trimmed) return;
      onSend(trimmed);
      setValue('');
      setSuggestions([]);
      return;
    }

    // Shift+Enter or Ctrl+Enter → newline
    if (key.return && (key.shift || key.ctrl)) {
      setValue(v => v + '\n');
      return;
    }

    // Tab → autocomplete
    if (key.tab && suggestions.length > 0) {
      setValue(suggestions[selIdx]!.command + ' ');
      setSuggestions([]);
      setSelIdx(0);
      return;
    }

    // Navigate suggestions
    if (key.upArrow && suggestions.length > 0) {
      setSelIdx(i => Math.max(0, i - 1));
      return;
    }
    if (key.downArrow && suggestions.length > 0) {
      setSelIdx(i => Math.min(suggestions.length - 1, i + 1));
      return;
    }

    // Backspace
    if (key.backspace || key.delete) {
      setValue(v => {
        const next = v.slice(0, -1);
        updateSuggestions(next);
        return next;
      });
      return;
    }

    // Skip modifier-only keys
    if (!input || key.ctrl || key.meta || key.escape) return;

    const next = value + input;
    setValue(next);
    updateSuggestions(next);
  });

  const lines = value.split('\n');
  const placeholder = 'Message sentinel-ai…  (/ for commands, Shift+Enter for newline)';

  return (
    <Box flexDirection="column">
      {/* Slash command autocomplete panel */}
      {suggestions.length > 0 && (
        <Box
          flexDirection="column"
          borderStyle="round"
          borderColor={c.border}
          paddingX={1}
          marginLeft={2}
          marginBottom={0}
        >
          {suggestions.map((cmd, i) => (
            <Box key={cmd.command}>
              <Text color={i === selIdx ? c.accent : c.border}>{i === selIdx ? '▸ ' : '  '}</Text>
              <Box width={14}>
                <Text color={i === selIdx ? c.accent : c.foreground} bold={i === selIdx}>
                  {cmd.command}
                </Text>
              </Box>
              <Text color={c.muted}>{cmd.description}</Text>
            </Box>
          ))}
        </Box>
      )}

      {/* Separator */}
      <Box>
        <Text color={c.dimBorder}>{'─'.repeat(80)}</Text>
      </Box>

      {/* Input row */}
      <Box flexDirection="column" paddingX={1}>
        {value ? (
          lines.map((line, i) => (
            <Box key={i}>
              <Text color={c.accent}>{i === 0 ? '❯ ' : '  '}</Text>
              <Text color={c.foreground}>{line}</Text>
              {i === lines.length - 1 && <Text color={c.accent}>█</Text>}
            </Box>
          ))
        ) : (
          <Box>
            <Text color={c.accent}>{disabled ? '◌ ' : '❯ '}</Text>
            <Text color={c.muted} dimColor>{placeholder}</Text>
          </Box>
        )}
      </Box>
    </Box>
  );
}
