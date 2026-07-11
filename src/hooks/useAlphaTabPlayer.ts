import { AlphaTabApi, NotationElement, type synth } from '@coderline/alphatab';
import { type RefObject, useEffect, useRef } from 'react';
import { docUrl } from '@/lib/stream';

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
  tabFile: number | undefined,
) {
  const alphaTabRef = useRef<AlphaTabApi>(null);

  // biome-ignore lint/correctness/useExhaustiveDependencies: refs stables, lues une fois au montage
  useEffect(() => {
    const audio = audioElmt.current;

    if (!tabFile || !tabElmt.current) {
      return;
    }

    const instance = new AlphaTabApi(tabElmt.current, {
      core: {
        file: docUrl(tabFile),
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
      },
    });
    alphaTabRef.current = instance;

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
            audio.currentTime = (time * audio.playbackRate) / 1000;
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

  return alphaTabRef;
}
