import { useEffect, useRef, useState } from 'react';
import {
  type BackingInfo,
  type BackingStatus,
  backingLoad,
  backingPause,
  backingPlay,
  backingSeek,
  backingSetVolume,
  backingUnload,
  subscribeBackingStatus,
} from '@/lib/ipc';
import { createSerialQueue } from '@/lib/serial-queue';
import type { Provider } from '@/types/model';

const IDLE: BackingStatus = {
  positionMs: 0,
  durationMs: 0,
  playing: false,
  ended: false,
  loaded: false,
  sampleRate: 0,
  error: null,
};

export type BackingEngine = ReturnType<typeof useBackingEngine>;

// Chargements et déchargements du moteur strictement ordonnés, pour toutes les vues : sans ça,
// le déchargement envoyé en quittant une vue (ou par le double montage de React.StrictMode)
// pouvait arriver après le chargement de la vue suivante et l'invalider côté Rust — et un
// premier track lent à télécharger pouvait remplacer celui choisi ensuite.
const engineQueue = createSerialQueue();

// Backing track jouée par le moteur audio natif (horloge maître, ARCHITECTURE.md §7) — remplace
// l'ancienne balise <audio>. Le track est chargé entièrement côté Rust avant de pouvoir jouer.
export default function useBackingEngine(provider: Provider, fileId: string | undefined) {
  const status = useRef<BackingStatus>(IDLE);
  const onStatusRef = useRef<((s: BackingStatus) => void) | undefined>(undefined);
  const [info, setInfo] = useState<BackingInfo | undefined>(undefined);
  const [loading, setLoading] = useState(false);
  const [loadError, setLoadError] = useState<string | undefined>(undefined);
  const [engineError, setEngineError] = useState<string | null>(null);
  // Incrémenté quand la sortie change de fréquence : le moteur a alors perdu le track chargé.
  const [reloadKey, setReloadKey] = useState(0);

  useEffect(() => {
    let unsubscribe: (() => void) | undefined;
    let cancelled = false;

    subscribeBackingStatus((s) => {
      const prev = status.current;
      status.current = s;
      if (prev.loaded && prev.sampleRate !== 0 && s.sampleRate !== prev.sampleRate) {
        setReloadKey((k) => k + 1);
      }
      setEngineError(s.error);
      onStatusRef.current?.(s);
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
      // Le moteur est global et survit à l'écran : sans ça, la backing track continuerait de
      // jouer après avoir quitté la vue tablature.
      engineQueue(() => backingPause().then(backingUnload)).catch(() => {});
    };
  }, []);

  // biome-ignore lint/correctness/useExhaustiveDependencies: reloadKey force le rechargement
  useEffect(() => {
    if (!fileId) {
      return;
    }

    let cancelled = false;
    setLoading(true);
    setLoadError(undefined);
    setInfo(undefined);

    engineQueue(async () => (cancelled ? undefined : backingLoad(provider, fileId)))
      .then((loaded) => {
        if (!cancelled && loaded) {
          setInfo(loaded);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setLoadError(String(err));
        }
      })
      .finally(() => {
        if (!cancelled) {
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [provider, fileId, reloadKey]);

  return {
    status,
    info,
    loading,
    error: loadError ?? engineError ?? undefined,
    play: () => backingPlay(),
    pause: () => backingPause(),
    seek: (ms: number) => backingSeek(ms),
    setVolume: (gain: number) => backingSetVolume(gain),
    /** Un seul abonné : la boucle de synchro de la vue tablature. */
    onStatus: (cb: (s: BackingStatus) => void) => {
      onStatusRef.current = cb;
    },
  };
}
