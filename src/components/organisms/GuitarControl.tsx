import {
  ChevronDown,
  ExternalLink,
  Guitar,
  Plug,
  Power,
  Settings,
  TriangleAlert,
  Volume2,
  VolumeX,
} from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import Button from '@/components/atoms/Button';
import LevelMeter from '@/components/molecules/LevelMeter';
import { useAudioEngine } from '@/contexts/AudioEngineContext';
import cn from '@/lib/cn';
import { faderDb } from '@/lib/fader';

const STATE_BUTTON = {
  off: {
    cls: 'border-mix-(--accent)/55 text-fg [&_svg]:text-accent',
    icon: Guitar,
    label: "Configurer l'audio",
    title: 'Aucune carte son choisie',
  },
  stopped: {
    cls: 'border-mix-(--warn)/50 bg-mix-(--warn)/10 text-fg [&_svg]:text-warn',
    icon: Power,
    label: 'Audio arrêté',
    title: 'La carte son est arrêtée',
  },
  error: {
    cls: 'border-mix-(--danger)/55 bg-mix-(--danger)/12 text-fg [&_svg]:text-danger',
    icon: TriangleAlert,
    label: 'Audio : erreur',
    title: 'Erreur de la carte son',
  },
} as const;

/** Bloc plugin du popover : plugin chargé et son interface, sinon accès au choix d'un plugin. */
function PopoverPlugin({ onDone }: { onDone: () => void }) {
  const { status, openSettings, openPluginEditor } = useAudioEngine();
  const plugin = status?.plugin ?? null;
  const missing = status?.pluginMissing;
  let sub = "Aucun plugin d'ampli chargé";
  if (plugin) {
    sub = `${plugin.vendor} · VST3`;
  } else if (missing) {
    sub = `${missing} introuvable`;
  }

  return (
    <>
      <div className="mb-3 flex items-center gap-2.5">
        <span className="type-ic tab size-8">
          <Plug size={16} />
        </span>
        <div className="min-w-0 flex-1">
          <div className="truncate font-bold text-[13.5px]">
            {plugin ? plugin.name : 'Son direct'}
          </div>
          <div className="truncate text-[11.5px] text-fg2">{sub}</div>
        </div>
      </div>
      <Button
        className="mb-4 h-8.5 w-full text-[13px]"
        onClick={() => {
          (plugin ? openPluginEditor : openSettings)();
          onDone();
        }}
      >
        {plugin ? <ExternalLink size={15} /> : <Plug size={15} />}
        {plugin ? "Ouvrir l'interface du plugin" : 'Choisir un plugin'}
      </Button>
    </>
  );
}

/** Barre de titre : mute + vumètre d'entrée guitare, et accès rapide au volume et aux réglages. */
export default function GuitarControl() {
  const audio = useAudioEngine();
  const { status, settings, levels, openSettings, setMuted, setFader } = audio;
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const inputLevel = useCallback(() => levels.current.input, [levels]);
  const outputLevel = useCallback(() => levels.current.output, [levels]);

  useEffect(() => {
    if (!open) {
      return;
    }
    const outside = (e: MouseEvent) => {
      if (root.current && !root.current.contains(e.target as Node)) {
        setOpen(false);
      }
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        setOpen(false);
      }
    };
    document.addEventListener('mousedown', outside);
    document.addEventListener('keydown', onKey);

    return () => {
      document.removeEventListener('mousedown', outside);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  if (!status || !settings) {
    return null;
  }

  if (status.state !== 'running') {
    const st = STATE_BUTTON[status.state];
    const title = status.state === 'error' && status.error ? status.error : st.title;

    return (
      <button
        type="button"
        className={cn([
          'inline-flex h-6.5 items-center gap-1.5 whitespace-nowrap rounded-full border border-border bg-chip px-2.5 font-semibold text-[11.5px] text-fg2',
          st.cls,
        ])}
        onClick={openSettings}
        title={`${title} — ouvrir les réglages audio`}
      >
        <st.icon size={13} />
        <span>{st.label}</span>
      </button>
    );
  }

  const muted = settings.guitarMuted;

  const toggleMute = () => setMuted(!muted);
  const muteTitle = muted ? 'Réactiver la guitare' : 'Couper la guitare';

  return (
    <div ref={root} className="relative">
      <div
        className={cn([
          'flex h-6.5 items-center gap-1.5 rounded-full border border-border bg-chip px-0.5',
          open && 'border-border2',
        ])}
      >
        <button
          type="button"
          className={cn([
            'relative flex size-5.5 items-center justify-center rounded-full text-accent hover:bg-chip',
            muted &&
              'text-fg3 after:absolute after:h-[1.6px] after:w-4.25 after:-rotate-45 after:rounded-[1px] after:bg-danger',
          ])}
          onClick={toggleMute}
          aria-pressed={!muted}
          title={muteTitle}
        >
          <Guitar size={14} />
        </button>
        <LevelMeter get={inputLevel} length={34} thick={4} dim={muted} />
        <button
          type="button"
          className="flex size-5.5 items-center justify-center rounded-full text-fg2 hover:bg-chip hover:text-fg"
          onClick={() => setOpen((o) => !o)}
          aria-expanded={open}
          title="Accès rapide guitare"
        >
          <ChevronDown size={13} />
        </button>
      </div>

      {open && (
        <div
          className="popover top-[calc(100%+8px)] right-0 w-72.5"
          role="dialog"
          aria-label="Guitare"
        >
          <div className="mb-3 flex items-center justify-between">
            <span className="eyebrow text-[10px]">Guitare</span>
            <span className="mono flex items-center gap-1.5 text-[10.5px] text-fg3">
              <span className="size-1.5 rounded-full bg-done" />
              {status.sampleRate / 1000} kHz · {status.buffer ?? 'système'}
              {status.latencyMs !== null && ` · ${status.latencyMs.toFixed(1)} ms`}
            </span>
          </div>

          <PopoverPlugin onDone={() => setOpen(false)} />

          <div className="mb-1.5 flex items-center justify-between">
            <span className="font-semibold text-fg2 text-xs">Volume guitare</span>
            <span className="mono text-[11.5px] text-fg2">
              {muted ? 'Coupé' : faderDb(settings.guitarFader)}
            </span>
          </div>
          <div className="flex items-center gap-2">
            <button
              type="button"
              className={cn(['mute-btn', muted && 'on'])}
              onClick={toggleMute}
              title={muteTitle}
              aria-pressed={muted}
            >
              {muted ? <VolumeX size={14} /> : <Volume2 size={14} />}
            </button>
            <div className="flex flex-1 flex-col gap-1">
              <input
                type="range"
                className="fader"
                min="0"
                max="100"
                value={settings.guitarFader}
                onChange={(e) => setFader(+e.target.value)}
                aria-label="Volume guitare"
              />
              <LevelMeter get={outputLevel} length="100%" thick={3} />
            </div>
          </div>

          <button
            type="button"
            className="pop-link"
            onClick={() => {
              openSettings();
              setOpen(false);
            }}
          >
            <Settings size={14} /> Réglages audio…
          </button>
        </div>
      )}
    </div>
  );
}
