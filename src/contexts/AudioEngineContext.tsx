import {
  createContext,
  type RefObject,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
} from 'react';
import { useToast } from '@/contexts/ToastContext';
import {
  type AudioSettings,
  type AudioStatus,
  audioConfigure,
  audioDevices,
  audioResetXruns,
  audioSetEnabled,
  audioSettings,
  type DuplexConfig,
  guitarSetFader,
  guitarSetMuted,
  type HostDevices,
  type PluginFolders,
  pluginAddFolder,
  pluginFolders,
  pluginLoad,
  pluginOpenEditor,
  pluginRemoveFolder,
  pluginScan,
  pluginUnload,
  subscribeAudioStatus,
} from '@/lib/ipc';

export type GuitarLevels = { input: number; output: number };

interface AudioEngineValue {
  /** null tant que le premier statut n'est pas arrivé. */
  status: AudioStatus | null;
  /** Crêtes du dernier statut, lues hors React par les vumètres (pas de rendu à 30 Hz). */
  levels: RefObject<GuitarLevels>;
  settings: AudioSettings | null;
  devices: HostDevices[] | null;
  devicesLoading: boolean;
  /** Une reconfiguration de la carte est en cours. */
  restarting: boolean;
  refreshDevices: () => void;
  configure: (duplex: DuplexConfig | null) => void;
  setEnabled: (enabled: boolean) => void;
  setFader: (value: number) => void;
  setMuted: (muted: boolean) => void;
  resetXruns: () => void;
  /** Dossiers VST3 (standard + ajoutés) ; null tant qu'ils ne sont pas lus. */
  folders: PluginFolders | null;
  scanning: boolean;
  /** Identifiant du plugin en cours de chargement. */
  loadingPlugin: string | null;
  scanPlugins: () => void;
  loadPlugin: (id: string) => void;
  unloadPlugin: () => void;
  openPluginEditor: () => void;
  addFolder: () => void;
  removeFolder: (path: string) => void;
  settingsOpen: boolean;
  openSettings: () => void;
  closeSettings: () => void;
}

const AudioEngineContext = createContext<AudioEngineValue | undefined>(undefined);

/** Égalité du statut hors vumètres : seuls ces champs-là provoquent un rendu. */
function sameStatus(a: AudioStatus | null, b: AudioStatus) {
  return (
    a !== null &&
    a.state === b.state &&
    a.error === b.error &&
    a.sampleRate === b.sampleRate &&
    a.buffer === b.buffer &&
    a.latencyMs === b.latencyMs &&
    a.xruns === b.xruns &&
    a.deviceName === b.deviceName &&
    a.plugin?.id === b.plugin?.id &&
    a.pluginMissing === b.pluginMissing
  );
}

/**
 * Envoie la dernière valeur demandée, une requête à la fois : un fader déplacé à la souris
 * produit des dizaines de valeurs par seconde, chacune persistée côté Rust.
 */
function useLatestSender<T>(send: (value: T) => Promise<void>) {
  const pending = useRef<{ value: T } | null>(null);
  const busy = useRef(false);

  return useCallback(
    (value: T) => {
      pending.current = { value };
      if (busy.current) {
        return;
      }
      busy.current = true;
      const loop = async () => {
        while (pending.current) {
          const next = pending.current.value;
          pending.current = null;
          await send(next).catch((err) => console.error('audio :', err));
        }
        busy.current = false;
      };
      loop();
    },
    [send],
  );
}

export function AudioEngineProvider({ children }: { children: React.ReactNode }) {
  const [status, setStatus] = useState<AudioStatus | null>(null);
  const levels = useRef<GuitarLevels>({ input: 0, output: 0 });
  const [settings, setSettings] = useState<AudioSettings | null>(null);
  const [devices, setDevices] = useState<HostDevices[] | null>(null);
  const [devicesLoading, setDevicesLoading] = useState(false);
  const [restarting, setRestarting] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [folders, setFolders] = useState<PluginFolders | null>(null);
  const [scanning, setScanning] = useState(false);
  const [loadingPlugin, setLoadingPlugin] = useState<string | null>(null);
  const { showToast } = useToast();

  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    let cancelled = false;

    audioSettings().then((s) => {
      if (!cancelled) {
        setSettings(s);
      }
    });
    subscribeAudioStatus((s) => {
      levels.current = { input: s.inputLevel, output: s.outputLevel };
      setStatus((prev) => (sameStatus(prev, s) ? prev : s));
    }).then((unsub) => {
      if (cancelled) {
        unsub();
      } else {
        unsubscribe = unsub;
      }
    });

    return () => {
      cancelled = true;
      unsubscribe?.();
    };
  }, []);

  const refreshDevices = useCallback(() => {
    setDevicesLoading(true);
    audioDevices()
      .then(setDevices)
      .catch((err) => console.error('audio : inventaire des cartes :', err))
      .finally(() => setDevicesLoading(false));
  }, []);

  /** Reconfiguration de la carte : bloquante côté Rust le temps de rouvrir les flux. */
  const reopen = useCallback((change: Partial<AudioSettings>, run: () => Promise<void>) => {
    setSettings((s) => (s ? { ...s, ...change } : s));
    setRestarting(true);
    run()
      .catch((err) => console.error('audio :', err))
      .finally(() => setRestarting(false));
  }, []);

  /** Erreur d'une commande plugin : montrée à l'utilisateur (chargement raté, pas d'interface…). */
  const fail = useCallback(
    (err: unknown) => showToast(err instanceof Error ? err.message : String(err), 'error'),
    [showToast],
  );

  const scanPlugins = useCallback(() => {
    setScanning(true);
    pluginScan()
      .then((scan) => setSettings((s) => (s ? { ...s, scan } : s)))
      .catch(fail)
      .finally(() => setScanning(false));
  }, [fail]);

  const refreshFolders = useCallback(() => {
    pluginFolders().then(setFolders).catch(fail);
  }, [fail]);

  const sendFader = useLatestSender(guitarSetFader);
  const sendMuted = useLatestSender(guitarSetMuted);

  // Actions à identité stable : un composant qui en dépend dans un effet (le tiroir rafraîchit
  // l'inventaire à son ouverture) ne doit pas se relancer à chaque statut reçu.
  const actions = useMemo(
    () => ({
      refreshDevices,
      configure: (duplex: DuplexConfig | null) =>
        reopen({ duplex, engineEnabled: true }, () => audioConfigure(duplex)),
      setEnabled: (enabled: boolean) =>
        reopen({ engineEnabled: enabled }, () => audioSetEnabled(enabled)),
      setFader: (fader: number) => {
        setSettings((s) => (s ? { ...s, guitarFader: fader } : s));
        sendFader(fader);
      },
      setMuted: (muted: boolean) => {
        setSettings((s) => (s ? { ...s, guitarMuted: muted } : s));
        sendMuted(muted);
      },
      resetXruns: () => {
        audioResetXruns();
      },
      scanPlugins,
      loadPlugin: (id: string) => {
        setLoadingPlugin(id);
        pluginLoad(id)
          .then((plugin) => setSettings((s) => (s ? { ...s, plugin } : s)))
          .catch(fail)
          .finally(() => setLoadingPlugin(null));
      },
      unloadPlugin: () => {
        setSettings((s) => (s ? { ...s, plugin: null } : s));
        pluginUnload().catch(fail);
      },
      openPluginEditor: () => {
        pluginOpenEditor().catch(fail);
      },
      // Un dossier ajouté est scanné aussitôt (maquette).
      addFolder: () => {
        pluginAddFolder()
          .then((added) => {
            refreshFolders();
            if (added) {
              scanPlugins();
            }
          })
          .catch(fail);
      },
      removeFolder: (path: string) => {
        pluginRemoveFolder(path).then(refreshFolders).catch(fail);
      },
      openSettings: () => {
        refreshFolders();
        setSettingsOpen(true);
      },
      closeSettings: () => setSettingsOpen(false),
    }),
    [refreshDevices, reopen, sendFader, sendMuted, scanPlugins, refreshFolders, fail],
  );

  const value = useMemo<AudioEngineValue>(
    () => ({
      status,
      levels,
      settings,
      devices,
      devicesLoading,
      restarting,
      settingsOpen,
      folders,
      scanning,
      loadingPlugin,
      ...actions,
    }),
    [
      status,
      settings,
      devices,
      devicesLoading,
      restarting,
      settingsOpen,
      folders,
      scanning,
      loadingPlugin,
      actions,
    ],
  );

  return <AudioEngineContext.Provider value={value}>{children}</AudioEngineContext.Provider>;
}

export function useAudioEngine() {
  const ctx = useContext(AudioEngineContext);
  if (!ctx) {
    throw new Error('useAudioEngine must be used within an AudioEngineProvider');
  }

  return ctx;
}
