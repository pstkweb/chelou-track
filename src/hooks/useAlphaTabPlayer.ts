import { AlphaTabApi, NotationElement, type synth } from '@coderline/alphatab';
import { type RefObject, useEffect, useRef, useState } from 'react';
import { docUrl } from '@/lib/stream';
import type { Provider } from '@/types/model';

function cssVar(name: string) {
  return getComputedStyle(document.documentElement).getPropertyValue(name).trim();
}

// couleurs AlphaTab dérivées des tokens du thème courant (globals.css)
function alphaTabResources() {
  return {
    mainGlyphColor: cssVar('--text'),
    secondaryGlyphColor: cssVar('--text-3'),
    staffLineColor: cssVar('--border-2'),
    barSeparatorColor: cssVar('--text-2'),
    barNumberColor: cssVar('--accent'),
    scoreInfoColor: cssVar('--text-3'),
  };
}

export default function useAlphaTabPlayer(
  tabElmt: RefObject<HTMLDivElement | null>,
  audioElmt: RefObject<HTMLAudioElement | null>,
  tabFile: string | undefined,
  provider: Provider,
) {
  const alphaTabRef = useRef<AlphaTabApi>(null);
  const beatsPerBarRef = useRef(4);
  const notatedBpmRef = useRef(120);
  // Copie réactive de notatedBpmRef, pour les usages côté rendu (affichage, arguments de
  // hooks) — la ref seule ne déclenche pas de re-render quand le score se charge.
  const [notatedBpm, setNotatedBpm] = useState(120);
  const scoreLoadedRef = useRef<Promise<void>>(new Promise(() => {}));
  // Renseignés par l'appelant (TabScreen) à chaque changement de backing track — lus par
  // seekTo() ci-dessous pour convertir un temps "noté" (domaine AlphaTab) en temps audio réel.
  const trackBpmRef = useRef<number | undefined>(undefined);
  const leadInMsRef = useRef<number | undefined>(undefined);

  // biome-ignore lint/correctness/useExhaustiveDependencies: refs stables, lues une fois au montage
  useEffect(() => {
    const audio = audioElmt.current;

    if (!tabFile || !tabElmt.current) {
      return;
    }

    const instance = new AlphaTabApi(tabElmt.current, {
      core: {
        file: docUrl(provider, tabFile),
        fontDirectory: '/font/',
      },
      display: {
        resources: alphaTabResources(),
      },
      notation: {
        elements: new Map([
          [NotationElement.ScoreTitle, false],
          [NotationElement.ScoreSubTitle, false],
          [NotationElement.ScoreArtist, false],
          [NotationElement.ScoreAlbum, false],
          [NotationElement.ScoreWords, false],
          [NotationElement.ScoreMusic, false],
          [NotationElement.ScoreWordsAndMusic, false],
          [NotationElement.ScoreCopyright, false],
        ]),
      },
      player: {
        playerMode: 'EnabledExternalMedia',
        enableCursor: true,
      },
    });
    alphaTabRef.current = instance;

    let resolveScoreLoaded: () => void = () => {};
    scoreLoadedRef.current = new Promise((resolve) => {
      resolveScoreLoaded = resolve;
    });
    instance.scoreLoaded.on((score) => {
      beatsPerBarRef.current = score.masterBars[0]?.timeSignatureNumerator ?? 4;
      notatedBpmRef.current = score.tempo || 120;
      setNotatedBpm(score.tempo || 120);
      resolveScoreLoaded();
    });

    if (audio) {
      // le handler externe ne peut être branché qu'une fois le player interne prêt
      // (instance.player reste null tant que _player.instance n'est pas initialisé)
      instance.playerReady.on(() => {
        const player = instance.player;

        if (!player) {
          return;
        }

        (player.output as unknown as synth.IExternalMediaSynthOutput).handler = {
          get backingTrackDuration() {
            const duration = audio.duration;

            return Number.isFinite(duration) ? duration * 1000 : 0;
          },
          get playbackRate() {
            return audio.playbackRate ?? 1;
          },
          set playbackRate(value) {
            audio.playbackRate = value;
          },
          get masterVolume() {
            return audio.volume ?? 100;
          },
          set masterVolume(value) {
            audio.volume = value;
          },
          seekTo(time) {
            const trackBpm = trackBpmRef.current;
            const leadInMs = leadInMsRef.current;

            if (time <= 0 || trackBpm === undefined || leadInMs === undefined) {
              // Seek au tout début (ou lead-in pas encore connu) : redémarre depuis le
              // vrai début du fichier (silence + count-in inclus), comme une première
              // lecture — pas juste le premier temps noté (qui correspondrait à leadInMs).
              audio.currentTime = 0;
              return;
            }

            // Inverse de la mise à l'échelle faite dans TabScreen.onTimeUpdate :
            // notatedMs = (audioMs - leadInMs) * (trackBpm / notatedBpm)
            const scale = trackBpm / notatedBpmRef.current;
            audio.currentTime = (time / scale + leadInMs) / 1000;
          },
          play() {
            audio.play();
          },
          pause() {
            audio.pause();
          },
        } as synth.IExternalMediaHandler;
      });
    }

    // suit les changements de thème (attribut data-mode / classe .dark sur <html>)
    const themeObserver = new MutationObserver(() => {
      instance.settings.fillFromJson({ display: { resources: alphaTabResources() } });
      instance.updateSettings();
      instance.render();
    });
    themeObserver.observe(document.documentElement, {
      attributes: true,
      attributeFilter: ['data-mode', 'class'],
    });

    return () => {
      themeObserver.disconnect();
      instance.destroy();
    };
  }, [tabFile]);

  return {
    alphaTabRef,
    beatsPerBarRef,
    notatedBpmRef,
    notatedBpm,
    scoreLoadedRef,
    trackBpmRef,
    leadInMsRef,
  };
}
