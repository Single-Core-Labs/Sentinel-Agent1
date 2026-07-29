import { Box, Text, useInput } from 'ink';
import { useEffect, useState, useRef } from 'react';
import type { ThemeConfig } from '../theme.js';

// ── Particle field ─────────────────────────────────────────────────

interface Particle {
  x: number;
  y: number;
  char: string;
  age: number;
  maxAge: number;
  col: string;
}

const COLS = ['#F97316','#0EA5E9','#A78BFA','#22C55E','#E2E8F0','#64748B'];
const W = 30;
const H = 5;
const MAX_PARTICLES = 15;

function makeParticle(chars: string[]): Particle {
  return {
    x: Math.floor(Math.random() * W),
    y: Math.floor(Math.random() * H),
    char: chars[Math.floor(Math.random() * chars.length)]!,
    age: 0,
    maxAge: 8 + Math.floor(Math.random() * 14),
    col: COLS[Math.floor(Math.random() * COLS.length)]!,
  };
}

const WORDMARK_LINES = [
  '   _____ ______ _   _ _______ _____ _   _ ______ _      ',
  '  / ____|  ____| \\ | |__   __|_   _| \\ | |  ____| |     ',
  ' | (___ | |__  |  \\| |  | |    | | |  \\| | |__  | |     ',
  '  \\___ \\|  __| | . ` |  | |    | | | . ` |  __| | |     ',
  '  ____) | |____| |\\  |  | |   _| |_| |\\  | |____| |____ ',
  ' |_____/|______|_| \\_|  |_|  |_____|_| \\_|______|______|',
];

const BOOT_LINES = [
  'Checking system dependencies...',
  'Loading tool registry...',
  'Connecting session store...',
  'Ready.',
];

interface Props {
  onComplete: () => void;
  theme: ThemeConfig;
}

export function StartupSequence({ onComplete, theme }: Props) {
  const [phase, setPhase] = useState<'particles' | 'boot' | 'done'>('particles');
  const [particles, setParticles] = useState<Particle[]>([]);
  const [bootIndex, setBootIndex] = useState(0);
  const skipped = useRef(false);
  const c = theme.colors;

  // Skip on any key
  useInput(() => {
    if (skipped.current) return;
    skipped.current = true;
    setPhase('done');
    onComplete();
  });

  // Particle animation tick
  useEffect(() => {
    if (phase !== 'particles') return;
    const interval = setInterval(() => {
      setParticles(prev => {
        const next = prev
          .map(p => ({ ...p, age: p.age + 1 }))
          .filter(p => p.age < p.maxAge);
        while (next.length < MAX_PARTICLES) {
          next.push(makeParticle(theme.particleChars));
        }
        return next;
      });
    }, 200);

    // Advance to boot phase after 2.5s
    const advance = setTimeout(() => {
      if (skipped.current) return;
      setPhase('boot');
    }, 2500);

    return () => { clearInterval(interval); clearTimeout(advance); };
  }, [phase, theme.particleChars]);

  // Boot sequence stagger
  useEffect(() => {
    if (phase !== 'boot') return;
    if (bootIndex >= BOOT_LINES.length) {
      const t = setTimeout(() => { setPhase('done'); onComplete(); }, 400);
      return () => clearTimeout(t);
    }
    const t = setTimeout(() => setBootIndex(i => i + 1), bootIndex === 0 ? 400 : 500);
    return () => clearTimeout(t);
  }, [phase, bootIndex, onComplete]);

  // Render particle grid
  const renderGrid = () => {
    const grid: Record<string, Particle> = {};
    for (const p of particles) {
      grid[`${p.x},${p.y}`] = p;
    }
    const rows: JSX.Element[] = [];
    for (let y = 0; y < H; y++) {
      const cells: JSX.Element[] = [];
      for (let x = 0; x < W; x++) {
        const key = `${x},${y}`;
        const p = grid[key];
        const fade = p ? 1 - p.age / p.maxAge : 0;
        cells.push(
          <Text key={x} color={p?.col} dimColor={fade < 0.5}>
            {p ? p.char : ' '}
          </Text>
        );
      }
      rows.push(<Box key={y}>{cells}</Box>);
    }
    return rows;
  };

  if (phase === 'done') return null;

  if (phase === 'particles') {
    return (
      <Box flexDirection="column" paddingTop={1}>
        <Box flexDirection="column" alignItems="center">
          {renderGrid()}
        </Box>
        <Box flexDirection="column" alignItems="center" marginTop={1}>
          {WORDMARK_LINES.map((l, i) => (
            <Text key={i} color={c.accent} bold>{l}</Text>
          ))}
        </Box>
        <Box justifyContent="center" marginTop={1}>
          <Text color={c.muted} dimColor>Press any key to skip</Text>
        </Box>
      </Box>
    );
  }

  return (
    <Box flexDirection="column" paddingTop={1}>
      <Box flexDirection="column" alignItems="center">
        <Box flexDirection="column" alignItems="center">
          {WORDMARK_LINES.map((l, i) => (
            <Text key={i} color={c.accent} bold>{l}</Text>
          ))}
        </Box>
      </Box>
      <Box flexDirection="column" paddingTop={1} paddingLeft={3}>
        <Box marginBottom={1}>
          <Text color={c.accent} bold>◆ sentinel ai</Text>
          <Text color={c.muted}>  autonomous coding agent  v0.1</Text>
        </Box>
        {BOOT_LINES.slice(0, bootIndex).map((line, i) => {
          const isLast = i === BOOT_LINES.length - 1;
          return (
            <Box key={i}>
              <Text color={isLast ? c.success : c.muted}>{isLast ? '✓ ' : '  '}</Text>
              <Text color={isLast ? c.foreground : c.muted}>{line}</Text>
            </Box>
          );
        })}
      </Box>
    </Box>
  );
}
