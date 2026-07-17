import { useState, useEffect } from 'react';

/**
 * Universally prevents terminal shuttering by ensuring that spinners ONLY run
 * (and trigger React re-renders) when they are actively needed on screen.
 * 
 * @param frames The animation frames to cycle through
 * @param active Whether the spinner should currently be running. If false, the interval stops.
 * @param ms The speed of the spinner in milliseconds (default 250ms / 4fps)
 */
export function useSpinner(frames: string[], active: boolean, ms = 250) {
  const [i, setI] = useState(0);

  useEffect(() => {
    if (!active) return;
    
    const t = setInterval(() => setI(x => (x + 1) % frames.length), ms);
    return () => clearInterval(t);
  }, [frames, active, ms]);

  return frames[i] ?? '';
}
