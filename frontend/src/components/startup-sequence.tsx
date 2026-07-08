import { Box, Text, useInput } from 'ink';
import { useEffect, useState } from 'react';
import type { ThemeConfig } from '../theme.js';

// Clean wordmark — rendered as styled text, no box drawing
const WORDMARK = '◆ sentinel-ai';

const BOOT_LINES = [
  'Loading tools...',
  'Connecting session store...',
  'Ready.',
];

interface Props {
  onComplete: () => void;
  theme: ThemeConfig;
}

export function StartupSequence({ onComplete, theme }: Props) {
  const [bootIndex, setBootIndex] = useState(0);
  const [done, setDone] = useState(false);

  // Any keypress skips to completion
  useInput(() => {
    if (!done) {
      setDone(true);
      onComplete();
    }
  });

  useEffect(() => {
    if (done) return;
    if (bootIndex >= BOOT_LINES.length) {
      const t = setTimeout(() => {
        setDone(true);
        onComplete();
      }, 120);
      return () => clearTimeout(t);
    }
    const t = setTimeout(
      () => setBootIndex(i => i + 1),
      bootIndex === 0 ? 80 : 220,
    );
    return () => clearTimeout(t);
  }, [bootIndex, done, onComplete]);

  const c = theme.colors;

  return (
    <Box flexDirection="column" paddingTop={1} paddingLeft={2}>
      {/* Wordmark */}
      <Box marginBottom={1}>
        <Text color={c.accent} bold>
          {WORDMARK}
        </Text>
        <Text color={c.muted}> — platform engineering agent</Text>
      </Box>

      {/* Boot lines */}
      {BOOT_LINES.slice(0, bootIndex).map((line, i) => {
        const isLast = i === BOOT_LINES.length - 1;
        return (
          <Box key={i}>
            <Text color={isLast ? c.success : c.muted}>
              {isLast ? '✓ ' : '  '}
            </Text>
            <Text color={isLast ? c.foreground : c.muted}>{line}</Text>
          </Box>
        );
      })}
    </Box>
  );
}
