import {
  ExternalLink,
  Guitar,
  Metronome,
  Music,
  Plug,
  Settings,
  SlidersHorizontal,
  VolumeX,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import LevelMeter from '@/components/molecules/LevelMeter';
import { useAudioEngine } from '@/contexts/AudioEngineContext';
import cn from '@/lib/cn';
import { faderDb } from '@/lib/fader';

type Channel = {
  id: string;
  name: string;
  icon: typeof Music;
  value: number;
  muted: boolean;
  disabled?: boolean;
  sub?: string;
  warn?: boolean;
  get: () => number;
  onChange: (value: number) => void;
  onMute: () => void;
  action?: { icon: typeof Music; title: string; run: () => void };
};

const GUITAR_OFF = {
  off: 'Non configurée',
  error: 'Erreur audio',
  stopped: 'Audio arrêté',
} as const;

/** Voies Backing, Guitare et Clic, branchées sur l'état global du moteur (maquette `TabMixer`). */
function useChannels(): Channel[] | null {
  const audio = useAudioEngine();
  const { status, settings, levels } = audio;
  const backingLevel = useCallback(() => levels.current.backing, [levels]);
  const guitarLevel = useCallback(() => levels.current.output, [levels]);
  const clickLevel = useCallback(() => levels.current.click, [levels]);
  if (!status || !settings) {
    return null;
  }
  const live = status.state === 'running';
  const plugin = status.plugin;
  const guitarAction = () => {
    if (!live) {
      return { icon: Settings, title: 'Réglages audio', run: audio.openSettings };
    }
    if (plugin) {
      return {
        icon: ExternalLink,
        title: `Ouvrir l'interface de ${plugin.name}`,
        run: audio.openPluginEditor,
      };
    }
    return { icon: Plug, title: 'Choisir un plugin', run: audio.openSettings };
  };

  return [
    {
      id: 'backing',
      name: 'Backing',
      icon: Music,
      value: settings.backingFader,
      muted: settings.backingMuted,
      get: backingLevel,
      onChange: (v) => audio.setBackingMix(v, settings.backingMuted),
      onMute: () => audio.setBackingMix(settings.backingFader, !settings.backingMuted),
    },
    {
      id: 'guitar',
      name: 'Guitare',
      icon: Guitar,
      value: settings.guitarFader,
      muted: settings.guitarMuted,
      disabled: !live,
      sub: live
        ? (plugin?.name ?? 'Son direct')
        : GUITAR_OFF[status.state as keyof typeof GUITAR_OFF],
      warn: !live,
      get: guitarLevel,
      onChange: audio.setFader,
      onMute: () => audio.setMuted(!settings.guitarMuted),
      action: guitarAction(),
    },
    {
      id: 'click',
      name: 'Clic',
      icon: Metronome,
      value: settings.clickFader,
      muted: settings.clickMuted,
      get: clickLevel,
      onChange: (v) => audio.setClickMix(v, settings.clickMuted),
      onMute: () => audio.setClickMix(settings.clickFader, !settings.clickMuted),
    },
  ];
}

function Strip({ ch, onAction }: { ch: Channel; onAction: () => void }) {
  const Icon = ch.muted ? VolumeX : ch.icon;
  const muteTitle = `${ch.muted ? 'Réactiver' : 'Couper'} ${ch.name.toLowerCase()}`;

  return (
    <div className={cn(['flex w-21 flex-col items-center', ch.disabled && 'opacity-60'])}>
      <div className="font-semibold text-[12.5px]">{ch.name}</div>
      <div
        className={cn([
          'min-h-3.5 w-full truncate text-center text-[10.5px] text-fg3',
          ch.warn && 'text-warn',
        ])}
        title={ch.sub}
      >
        {ch.sub ?? ' '}
      </div>
      <div className="my-2 flex h-33 items-stretch justify-center gap-2">
        <input
          type="range"
          className="fader fader-v"
          min="0"
          max="100"
          value={ch.value}
          disabled={ch.disabled}
          onChange={(e) => ch.onChange(+e.target.value)}
          aria-label={`Volume ${ch.name}`}
        />
        <LevelMeter get={ch.get} orient="v" length={132} thick={6} dim={ch.muted} />
      </div>
      <div className="mono mb-2 text-[10.5px] text-fg2">
        {ch.muted ? 'Coupé' : faderDb(ch.value)}
      </div>
      <div className="flex justify-center gap-1">
        <button
          type="button"
          className={cn(['mute-btn size-7', ch.muted && 'on'])}
          onClick={ch.onMute}
          disabled={ch.disabled}
          aria-pressed={ch.muted}
          title={muteTitle}
        >
          <Icon size={13} />
        </button>
        {ch.action && (
          <button
            type="button"
            className="mute-btn size-7"
            onClick={() => {
              onAction();
              ch.action?.run();
            }}
            title={ch.action.title}
            aria-label={ch.action.title}
          >
            <ch.action.icon size={14} />
          </button>
        )}
      </div>
    </div>
  );
}

/** Mixer de la vue tablature : bouton « Mixer » (mini-vumètres, pastille si une voie est coupée)
 * qui déplie trois tranches — Backing, Guitare, Clic. Réglages globaux et persistés. */
export default function TabMixer() {
  const channels = useChannels();
  const { openSettings } = useAudioEngine();
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) {
      return;
    }
    const outside = (e: MouseEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', outside);

    return () => document.removeEventListener('mousedown', outside);
  }, [open]);

  if (!channels) {
    return null;
  }
  const anyMuted = channels.some((c) => c.muted);

  return (
    <div ref={root} className="relative flex items-center gap-1.5">
      <button
        type="button"
        className={cn([
          'inline-flex h-9 items-center gap-2 rounded-md border border-border bg-chip px-3 font-semibold text-[13px] text-fg2 hover:text-fg',
          open && 'border-border2 text-fg',
        ])}
        onClick={() => setOpen((o) => !o)}
        aria-expanded={open}
        title="Mixer"
      >
        <SlidersHorizontal size={16} />
        <span>Mixer</span>
        <span className="flex h-4.5 items-end gap-0.5">
          {channels.map((c) => (
            <LevelMeter
              key={c.id}
              get={c.get}
              orient="v"
              length={18}
              thick={3}
              segments={6}
              dim={c.muted || c.disabled === true}
            />
          ))}
        </span>
        {anyMuted && (
          <span className="size-1.75 rounded-full bg-warn" title="Une voie est coupée" />
        )}
      </button>
      <button
        type="button"
        className="inline-flex size-9 items-center justify-center rounded-md border border-border bg-chip text-fg2 hover:text-fg"
        onClick={openSettings}
        title="Réglages audio"
        aria-label="Réglages audio"
      >
        <Settings size={16} />
      </button>
      {open && (
        <div
          className="popover right-0 bottom-[calc(100%+10px)] flex gap-2"
          role="dialog"
          aria-label="Mixer"
        >
          {channels.map((ch) => (
            <Strip key={ch.id} ch={ch} onAction={() => setOpen(false)} />
          ))}
        </div>
      )}
    </div>
  );
}
