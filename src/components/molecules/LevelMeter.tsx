import { useEffect, useRef } from 'react';
import cn from '@/lib/cn';

type LevelMeterProps = {
  /** Niveau 0-1, lu à chaque image (hors React). */
  get: () => number;
  /** Horizontal (défaut) ou vertical (mixer). */
  orient?: 'h' | 'v';
  /** Longueur en px, ou valeur CSS (`'100%'`). */
  length?: number | string;
  /** Nombre de segments ; défaut : un tous les 4 px. */
  segments?: number;
  thick?: number;
  dim?: boolean;
  className?: string;
};

type Ballistics = { level: number; held: number; heldAt: number };

/** Attaque immédiate, retombée lente ; crête maintenue 700 ms puis qui redescend. */
function follow(b: Ballistics, raw: number, now: number): Ballistics {
  const target = Math.max(0, Math.min(1, raw || 0));
  b.level = target > b.level ? target : b.level * 0.9 + target * 0.1;
  if (b.level >= b.held) {
    b.held = b.level;
    b.heldAt = now;
  } else if (now - b.heldAt > 700) {
    b.held = Math.max(b.level, b.held - 0.012);
  }

  return b;
}

/**
 * Vumètre segmenté (horizontal ou vertical) animé par `requestAnimationFrame` (style écrit directement, aucun
 * rendu React) : attaque immédiate, retombée lente, crête maintenue 700 ms.
 */
export default function LevelMeter({
  get,
  orient = 'h',
  length = 48,
  thick = 4,
  segments,
  dim = false,
  className,
}: LevelMeterProps) {
  const cover = useRef<HTMLElement>(null);
  const peak = useRef<HTMLElement>(null);

  useEffect(() => {
    let raf = 0;
    const ballistics = { level: 0, held: 0, heldAt: 0 };
    const tick = (now: number) => {
      const { level, held } = follow(ballistics, get(), now);
      const side = orient === 'h' ? 'left' : 'bottom';
      if (cover.current) {
        cover.current.style[side] = `${level * 100}%`;
      }
      if (peak.current) {
        peak.current.style[side] = `calc(${held * 100}% - 2px)`;
        peak.current.style.opacity = held > 0.02 ? '1' : '0';
      }
      raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);

    return () => cancelAnimationFrame(raf);
  }, [get, orient]);

  const horizontal = orient === 'h';
  const count = segments ?? (typeof length === 'number' ? Math.max(4, Math.round(length / 4)) : 24);

  return (
    <div
      className={cn(['lvl', !horizontal && 'lvl-v', dim && 'lvl-dim', className])}
      aria-hidden="true"
      style={
        {
          width: horizontal ? length : thick,
          height: horizontal ? thick : length,
          '--seg': `${100 / count}%`,
        } as React.CSSProperties
      }
    >
      <i ref={cover} className="lvl-cover" />
      <b ref={peak} className="lvl-peak" />
    </div>
  );
}
