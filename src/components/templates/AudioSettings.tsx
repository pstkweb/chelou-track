import { Check, Clock, Cpu, Power, RefreshCw, TriangleAlert, X } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import Button from '@/components/atoms/Button';
import Chip from '@/components/atoms/Chip';
import IconButton from '@/components/atoms/IconButton';
import SectionTitle from '@/components/atoms/SectionTitle';
import Spinner from '@/components/atoms/Spinner';
import LevelMeter from '@/components/molecules/LevelMeter';
import PluginSection from '@/components/organisms/PluginSection';
import { useAudioEngine } from '@/contexts/AudioEngineContext';
import cn from '@/lib/cn';
import type { AudioHost, DeviceInfo, DuplexConfig, HostDevices } from '@/lib/ipc';

const HOST_LABELS: Record<AudioHost, string> = {
  asio: 'ASIO',
  wasapi: 'WASAPI',
  coreaudio: 'CoreAudio',
  alsa: 'ALSA',
};
const BUFFER_SIZES = [32, 64, 128, 256, 512, 1024];
const DEFAULT_RATE = 48000;
const DEFAULT_BUFFER = 128;

const SELECT =
  'h-8.5 w-full appearance-none rounded-md border border-border2 bg-bg bg-[url("data:image/svg+xml,%3Csvg_xmlns=%27http://www.w3.org/2000/svg%27_width=%2712%27_height=%2712%27_viewBox=%270_0_24_24%27_fill=%27none%27_stroke=%27%238b939d%27_stroke-width=%272%27_stroke-linecap=%27round%27%3E%3Cpath_d=%27m6_9_6_6_6-6%27/%3E%3C/svg%3E")] bg-position-[right_10px_center] bg-no-repeat pr-8 pl-2.5 text-[13px] text-fg outline-none focus:border-accent disabled:opacity-45 dark:scheme-dark';

function SetRow({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string | undefined;
  children: React.ReactNode;
}) {
  return (
    <div className="grid grid-cols-[minmax(110px,170px)_minmax(0,1fr)] items-center gap-x-4 gap-y-1.5 py-2 max-[520px]:grid-cols-1">
      <div className="flex flex-col font-semibold text-[13px] text-fg">
        {label}
        {hint && <span className="font-normal text-[11.5px] text-fg3">{hint}</span>}
      </div>
      <div className="min-w-0">{children}</div>
    </div>
  );
}

function Segmented<T extends string | number>({
  options,
  value,
  onChange,
  disabled,
}: {
  options: { value: T; label: string }[];
  value: T | undefined;
  onChange: (value: T) => void;
  disabled?: boolean;
}) {
  return (
    <div className="inline-flex flex-wrap gap-0.5 rounded-md border border-border bg-bg p-0.5">
      {options.map((o) => (
        <button
          type="button"
          key={o.value}
          aria-pressed={o.value === value}
          disabled={disabled}
          className={cn([
            'h-7 rounded-sm px-2.5 font-semibold text-[12.5px] text-fg2 tabular-nums enabled:hover:text-fg disabled:opacity-45',
            o.value === value && 'bg-accent text-accentink enabled:hover:text-accentink',
          ])}
          onClick={() => onChange(o.value)}
        >
          {o.label}
        </button>
      ))}
    </div>
  );
}

function EngineBar() {
  const { status, settings, restarting, setEnabled } = useAudioEngine();
  if (!status || !settings) {
    return null;
  }
  const st = restarting ? 'restarting' : status.state;
  const running = `${status.deviceName} · ${status.sampleRate / 1000} kHz · ${
    status.buffer === null ? 'buffer système' : `${status.buffer} échantillons`
  }`;
  const map = {
    running: { dot: 'bg-done', title: 'Carte son active', sub: running },
    restarting: {
      dot: 'bg-accent',
      title: 'Redémarrage de la carte…',
      sub: 'Application des nouveaux réglages',
    },
    stopped: { dot: 'bg-warn', title: 'Carte son arrêtée', sub: 'Ta guitare n’est pas entendue.' },
    error: { dot: 'bg-danger', title: 'Erreur de la carte son', sub: status.error },
    off: {
      dot: 'bg-fg3',
      title: 'Carte son non configurée',
      sub: 'Choisis une carte son pour entendre ta guitare.',
    },
  }[st];
  const small = 'h-8 px-3 text-[12.5px]';

  return (
    <div
      className={cn([
        'flex items-center gap-3 rounded-lg border border-border bg-bg3 px-3.5 py-3',
        st === 'error' &&
          'border-mix-(--danger)/55 bg-[color-mix(in_srgb,var(--danger)_9%,var(--bg-3))]',
        st === 'stopped' &&
          'border-mix-(--warn)/45 bg-[color-mix(in_srgb,var(--warn)_7%,var(--bg-3))]',
      ])}
    >
      {st === 'restarting' ? (
        <Spinner />
      ) : (
        <span className={cn(['size-2.5 flex-none rounded-full', map.dot])} />
      )}
      <div className="min-w-0 flex-1">
        <div className="font-bold text-[13.5px]">{map.title}</div>
        <div className="mt-px text-fg2 text-xs">{map.sub}</div>
      </div>
      {st === 'running' && (
        <Button className={small} onClick={() => setEnabled(false)}>
          <Power size={14} /> Arrêter
        </Button>
      )}
      {st === 'stopped' && (
        <Button variant="primary" className={small} onClick={() => setEnabled(true)}>
          <Power size={14} /> Démarrer
        </Button>
      )}
      {st === 'error' && settings.duplex && (
        <Button variant="primary" className={small} onClick={() => setEnabled(true)}>
          <RefreshCw size={14} /> Redémarrer
        </Button>
      )}
    </div>
  );
}

function Welcome({ configured, plugin }: { configured: boolean; plugin: boolean }) {
  const steps = [
    { done: configured, label: 'Choisis ta carte son et l’entrée de ta guitare' },
    { done: plugin, label: 'Charge ton plugin de simulation d’ampli' },
  ];

  return (
    <div className="rounded-lg border border-mix-(--accent)/45 bg-[color-mix(in_srgb,var(--accent)_7%,var(--bg-2))] p-4">
      <div className="display mb-1 text-lg">Entends ta guitare dans Chelou Track</div>
      <p className="mt-0 mb-3 text-[13px] text-fg2 leading-normal">
        Une seule fois : l'app s'en souviendra aux prochaines sessions.
      </p>
      <div className="flex flex-col gap-2">
        {steps.map((s, i) => (
          <div key={s.label} className="flex items-center gap-2.5 text-[13px]">
            <span
              className={cn([
                'flex size-5 flex-none items-center justify-center rounded-full border-[1.5px] border-border2 font-bold text-[11px] text-fg2',
                s.done && 'border-transparent bg-accent text-accentink',
              ])}
            >
              {s.done ? <Check size={12} /> : i + 1}
            </span>
            <span className={s.done ? 'text-fg2 line-through' : 'text-fg'}>{s.label}</span>
          </div>
        ))}
      </div>
    </div>
  );
}

/** Carte utilisable en duplex : entrée et sortie (ASIO, CoreAudio, ALSA). */
const isDuplex = (d: DeviceInfo) => d.inputs > 0 && d.outputs > 0;

/** Nouvelle configuration pour une carte choisie : garde ce qui reste valide. */
function configFor(
  host: AudioHost,
  input: DeviceInfo,
  output: DeviceInfo,
  prev: DuplexConfig | null,
): DuplexConfig {
  const rates = output.sampleRates;
  const keepRate = prev && rates.includes(prev.sampleRate) ? prev.sampleRate : null;
  const buffer = prev?.buffer ?? DEFAULT_BUFFER;
  const min = output.bufferMin ?? 0;
  const max = output.bufferMax ?? Number.POSITIVE_INFINITY;

  return {
    host,
    inputDevice: input.name,
    outputDevice: output.name,
    inputChannel: prev && prev.inputChannel < input.inputs ? prev.inputChannel : 0,
    outputPair: prev && prev.outputPair * 2 + 1 < output.outputs ? prev.outputPair : 0,
    sampleRate:
      keepRate ?? (rates.includes(DEFAULT_RATE) ? DEFAULT_RATE : (rates[0] ?? DEFAULT_RATE)),
    buffer: Math.min(max, Math.max(min, buffer)),
  };
}

/** Liste des cartes, avec la carte configurée même si elle n'est plus détectée. */
function withMissing(devices: DeviceInfo[], name: string | undefined, inputs: boolean) {
  if (!name || devices.some((d) => d.name === name)) {
    return devices.map((d) => ({ device: d, missing: false }));
  }
  const ghost: DeviceInfo = {
    name,
    inputs: inputs ? 1 : 0,
    outputs: inputs ? 0 : 2,
    sampleRates: [],
    bufferMin: null,
    bufferMax: null,
  };
  return [...devices, ghost].map((d) => ({ device: d, missing: d === ghost }));
}

function DeviceSelect({
  value,
  options,
  placeholder,
  onChange,
}: {
  value: string | undefined;
  options: { device: DeviceInfo; missing: boolean }[];
  placeholder: string;
  onChange: (device: DeviceInfo) => void;
}) {
  return (
    <select
      className={SELECT}
      value={value ?? ''}
      onChange={(e) => {
        const picked = options.find((o) => o.device.name === e.target.value);
        if (picked && !picked.missing) {
          onChange(picked.device);
        }
      }}
    >
      {!value && (
        <option value="" disabled>
          {placeholder}
        </option>
      )}
      {options.map(({ device, missing }) => (
        <option key={device.name} value={device.name}>
          {device.name}
          {missing ? ' (déconnectée)' : ''}
        </option>
      ))}
    </select>
  );
}

function DeviceSection({
  hosts,
  host,
  onHost,
}: {
  hosts: HostDevices[];
  host: AudioHost | undefined;
  onHost: (host: AudioHost) => void;
}) {
  const { settings, configure } = useAudioEngine();
  // WASAPI : entrée et sortie choisies séparément tant que l'une des deux manque.
  const [wasapiDraft, setWasapiDraft] = useState<{
    input?: DeviceInfo | undefined;
    output?: DeviceInfo | undefined;
  }>({});
  if (!settings) {
    return null;
  }
  const config = settings.duplex?.host === host ? settings.duplex : null;
  const devices = hosts.find((h) => h.host === host)?.devices ?? [];
  const find = (name: string | undefined) => devices.find((d) => d.name === name);
  const input = find(config?.inputDevice);
  const output = find(config?.outputDevice);
  const wasapi = host === 'wasapi';

  const pickWasapi = (side: 'input' | 'output', device: DeviceInfo) => {
    const next = { input: input ?? wasapiDraft.input, output: output ?? wasapiDraft.output };
    next[side] = device;
    setWasapiDraft(next);
    if (next.input && next.output) {
      configure(configFor('wasapi', next.input, next.output, config));
    }
  };

  return (
    <section className="border-border border-t pt-4">
      <SectionTitle icon={Cpu}>Périphérique</SectionTitle>
      <SetRow
        label="Pilote"
        hint={hosts.some((h) => h.host === 'asio') ? 'ASIO recommandé' : undefined}
      >
        {hosts.length > 1 ? (
          <Segmented
            options={hosts.map((h) => ({ value: h.host, label: HOST_LABELS[h.host] }))}
            value={host}
            onChange={onHost}
          />
        ) : (
          <Chip>{host ? HOST_LABELS[host] : '—'}</Chip>
        )}
      </SetRow>

      {wasapi ? (
        <>
          <SetRow label="Entrée">
            <DeviceSelect
              value={config?.inputDevice ?? wasapiDraft.input?.name}
              options={withMissing(
                devices.filter((d) => d.inputs > 0),
                config?.inputDevice,
                true,
              )}
              placeholder="Choisir l'entrée de ta carte…"
              onChange={(d) => pickWasapi('input', d)}
            />
          </SetRow>
          <SetRow label="Sortie">
            <DeviceSelect
              value={config?.outputDevice ?? wasapiDraft.output?.name}
              options={withMissing(
                devices.filter((d) => d.outputs > 0),
                config?.outputDevice,
                false,
              )}
              placeholder="Choisir la sortie de ta carte…"
              onChange={(d) => pickWasapi('output', d)}
            />
          </SetRow>
        </>
      ) : (
        <SetRow label="Carte son">
          <DeviceSelect
            value={config?.inputDevice}
            options={withMissing(devices.filter(isDuplex), config?.inputDevice, true)}
            placeholder="Choisir une carte son…"
            onChange={(d) => host && configure(configFor(host, d, d, config))}
          />
        </SetRow>
      )}

      <ChannelRows config={config} input={input} output={wasapi ? undefined : output} />
    </section>
  );
}

/** Canal d'entrée de la guitare et, hors WASAPI (`output` absent), paire de sorties. */
function ChannelRows({
  config,
  input,
  output,
}: {
  config: DuplexConfig | null;
  input: DeviceInfo | undefined;
  output: DeviceInfo | undefined;
}) {
  const { configure, levels } = useAudioEngine();
  const inputLevel = useCallback(() => levels.current.input, [levels]);
  const inputs = Math.max(1, input?.inputs ?? 1);
  const pairs = Math.max(1, Math.floor((output?.outputs ?? 2) / 2));

  return (
    <>
      <SetRow label="Entrée guitare">
        <div className="flex items-center gap-3">
          <select
            className={cn([SELECT, 'flex-1'])}
            disabled={!(config && input)}
            value={config?.inputChannel ?? 0}
            onChange={(e) => config && configure({ ...config, inputChannel: +e.target.value })}
          >
            {Array.from({ length: inputs }, (_, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: canaux numérotés, liste stable
              <option key={i} value={i}>
                Entrée {i + 1}
              </option>
            ))}
          </select>
          <LevelMeter get={inputLevel} length={72} thick={6} />
        </div>
      </SetRow>
      {output && (
        <SetRow label="Sortie">
          <select
            className={SELECT}
            disabled={!config}
            value={config?.outputPair ?? 0}
            onChange={(e) => config && configure({ ...config, outputPair: +e.target.value })}
          >
            {Array.from({ length: pairs }, (_, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: paires numérotées, liste stable
              <option key={i} value={i}>
                Sorties {i * 2 + 1}–{i * 2 + 2}
              </option>
            ))}
          </select>
        </SetRow>
      )}
    </>
  );
}

function findDevice(hosts: HostDevices[], host: AudioHost, name: string) {
  return hosts.find((h) => h.host === host)?.devices.find((d) => d.name === name);
}

/** Tailles de buffer proposées, bornées par ce qu'accepte la carte. */
function buffersFor(device: DeviceInfo | undefined) {
  const min = device?.bufferMin ?? 0;
  const max = device?.bufferMax ?? Number.POSITIVE_INFINITY;

  return BUFFER_SIZES.filter((b) => min <= b && b <= max);
}

/** Latence aller-retour des réglages choisis ; inconnue en WASAPI (buffer système). */
function estimatedLatency(config: DuplexConfig | null) {
  if (!config || config.host === 'wasapi') {
    return null;
  }

  return (2 * config.buffer * 1000) / config.sampleRate;
}

function RateSelect({
  rates,
  value,
  disabled,
  onChange,
}: {
  rates: number[];
  value: number | undefined;
  disabled: boolean;
  onChange: (rate: number) => void;
}) {
  return (
    <select
      className={SELECT}
      disabled={disabled}
      value={value ?? ''}
      onChange={(e) => onChange(+e.target.value)}
    >
      {rates.map((r) => (
        <option key={r} value={r}>
          {(r / 1000).toLocaleString('fr-FR')} kHz
        </option>
      ))}
    </select>
  );
}

/** Buffer supérieur à conseiller en cas de coupures (sans objet en WASAPI). */
function bufferAdvice(
  config: DuplexConfig | null,
  buffers: number[],
  apply: (next: number) => void,
) {
  const next =
    config?.host === 'wasapi' ? undefined : buffers.find((b) => b > (config?.buffer ?? 0));
  if (!(config && next)) {
    return undefined;
  }

  return {
    next,
    extraMs: ((next - config.buffer) / config.sampleRate) * 2000,
    apply: () => apply(next),
  };
}

function PerfSection({ hosts }: { hosts: HostDevices[] }) {
  const { status, settings, configure, resetXruns } = useAudioEngine();
  if (!status || !settings) {
    return null;
  }
  const config = settings.duplex;
  const device = config ? findDevice(hosts, config.host, config.outputDevice) : undefined;
  const wasapi = config?.host === 'wasapi';
  const off = !config || !device;
  const rates = device?.sampleRates ?? [];
  const buffers = buffersFor(device);
  const rate = wasapi ? status.sampleRate : config?.sampleRate;
  // Latence : celle du moteur s'il tourne, sinon l'estimation des réglages choisis.
  const latency = status.latencyMs ?? estimatedLatency(config);
  const edit = (patch: Partial<DuplexConfig>) => config && configure({ ...config, ...patch });

  return (
    <section className="border-border border-t pt-4">
      <SectionTitle icon={Clock}>Performance</SectionTitle>
      <SetRow label="Fréquence d'échantillonnage" hint={wasapi ? 'imposée par Windows' : undefined}>
        <RateSelect
          rates={wasapi || off ? [rate ?? DEFAULT_RATE] : rates}
          value={rate}
          disabled={off || wasapi}
          onChange={(r) => edit({ sampleRate: r })}
        />
      </SetRow>
      <SetRow label="Taille du buffer" hint="échantillons">
        {wasapi ? (
          <span className="text-[13px] text-fg2">Système (non réglable en WASAPI)</span>
        ) : (
          <Segmented
            disabled={off}
            options={buffers.map((b) => ({ value: b, label: String(b) }))}
            value={config?.buffer}
            onChange={(b) => edit({ buffer: b })}
          />
        )}
      </SetRow>
      <SetRow label="Latence estimée">
        <Latency ms={latency} off={off} />
      </SetRow>
      <SetRow label="Coupures audio" hint="xruns, cette session">
        <Xruns
          count={status.xruns}
          onReset={resetXruns}
          advice={bufferAdvice(config, buffers, (next) => edit({ buffer: next }))}
        />
      </SetRow>
    </section>
  );
}

function latencyQuality(ms: number) {
  if (ms < 10) {
    return { label: 'Imperceptible', cls: 'text-done', dot: 'bg-done' };
  }
  if (ms < 20) {
    return { label: 'Correcte', cls: 'text-warn', dot: 'bg-warn' };
  }

  return { label: 'Perceptible', cls: 'text-danger', dot: 'bg-danger' };
}

/** Latence aller-retour (`null` : inconnue, WASAPI) répartie entrée/sortie, avec sa qualité. */
function Latency({ ms, off }: { ms: number | null; off: boolean }) {
  const quality = ms === null || off ? null : latencyQuality(ms);

  return (
    <div className={cn(['flex flex-wrap items-baseline gap-3', off && 'opacity-45'])}>
      <span className="mono font-semibold text-[22px] text-fg">
        {ms === null ? '—' : `${ms.toFixed(1)} ms`}
      </span>
      {ms !== null && (
        <span className="mono text-[11.5px] text-fg2">
          entrée {(ms / 2).toFixed(1)} + sortie {(ms / 2).toFixed(1)}
        </span>
      )}
      {quality && (
        <span
          className={cn([
            'inline-flex items-center gap-1.25 whitespace-nowrap font-semibold text-xs',
            quality.cls,
          ])}
        >
          <span className={cn(['size-1.5 rounded-full', quality.dot])} />
          {quality.label}
        </span>
      )}
    </div>
  );
}

/** Compteur de coupures ; à partir de 3, conseil de passer au buffer supérieur. */
function Xruns({
  count,
  onReset,
  advice,
}: {
  count: number;
  onReset: () => void;
  advice: { next: number; extraMs: number; apply: () => void } | undefined;
}) {
  const label =
    count === 0
      ? 'Aucune coupure — le son est propre.'
      : count === 1
        ? 'coupure détectée'
        : 'coupures détectées';
  const tone = count >= 3 ? 'text-danger' : count ? 'text-warn' : 'text-fg';

  return (
    <div className="flex flex-col gap-2">
      <div className="flex items-center gap-2.5">
        <span className={cn(['mono font-semibold text-[15px]', tone])}>{count}</span>
        <span className="text-fg2 text-xs">{label}</span>
        {count > 0 && (
          <button type="button" className="pop-link m-0 w-auto border-0 p-0" onClick={onReset}>
            Remettre à zéro
          </button>
        )}
      </div>
      {count >= 3 && advice && (
        <div className="flex flex-wrap items-center gap-x-2.5 gap-y-2 rounded-md border border-mix-(--warn)/40 bg-mix-(--warn)/10 px-3 py-2.5 text-[12.5px] text-fg leading-[1.45]">
          <TriangleAlert size={15} className="text-warn" />
          <span className="min-w-50 flex-1">
            Des craquements sont probables. Passe le buffer à <b>{advice.next}</b> : +
            {advice.extraMs.toFixed(1)} ms de latence, mais un son stable.
          </span>
          <Button className="h-7 px-2.5 text-xs" onClick={advice.apply}>
            Passer à {advice.next}
          </Button>
        </div>
      )}
    </div>
  );
}

/** Tiroir des réglages audio (carte son, performance) — global à l'app. */
export default function AudioSettings() {
  const { settingsOpen, closeSettings, settings, devices, devicesLoading, refreshDevices } =
    useAudioEngine();
  const [host, setHost] = useState<AudioHost | undefined>(undefined);

  useEffect(() => {
    if (!settingsOpen) {
      return;
    }
    refreshDevices();
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        closeSettings();
      }
    };
    document.addEventListener('keydown', onKey);

    return () => document.removeEventListener('keydown', onKey);
  }, [settingsOpen, refreshDevices, closeSettings]);

  if (!settingsOpen || !settings) {
    return null;
  }
  const hosts = devices ?? [];
  // Pilote affiché : celui choisi à l'écran, sinon celui de la carte configurée, sinon le premier.
  const shownHost = host ?? settings.duplex?.host ?? hosts[0]?.host;

  return (
    <div className="drawer-wrap" role="presentation" onMouseDown={closeSettings}>
      <aside
        className="drawer"
        role="dialog"
        aria-label="Réglages audio"
        onMouseDown={(e) => e.stopPropagation()}
      >
        <div className="flex items-center justify-between gap-3 border-border border-b px-6 py-4.5">
          <div>
            <div className="eyebrow mb-0.75 text-[10px]">Carte son · global</div>
            <h2 className="display m-0 text-[22px]">Réglages audio</h2>
          </div>
          <IconButton onClick={closeSettings} title="Fermer" className="border border-border">
            <X size={17} />
          </IconButton>
        </div>
        <div className="scroll flex min-h-0 flex-1 flex-col gap-4.5 px-6 pt-4.5 pb-8">
          {!settings.duplex && <Welcome configured={false} plugin={settings.plugin !== null} />}
          <EngineBar />
          {devices === null && devicesLoading ? (
            <div className="flex items-center gap-2.5 text-[13px] text-fg2">
              <Spinner /> Recherche des cartes son…
            </div>
          ) : (
            <>
              <DeviceSection hosts={hosts} host={shownHost} onHost={setHost} />
              <PerfSection hosts={hosts} />
            </>
          )}
          <PluginSection />
        </div>
      </aside>
    </div>
  );
}
