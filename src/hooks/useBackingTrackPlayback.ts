import type { AlphaTabApi, synth } from '@coderline/alphatab';
import { type RefObject, useRef, useState } from 'react';
import type { BackingTrack, Method } from '@/types/model';

export default function useBackingTrackPlayback(
  alphaTabRef: RefObject<AlphaTabApi | null>,
  audioElmt: RefObject<HTMLAudioElement | null>,
  leadInMsRef: RefObject<number | undefined>,
  notatedBpmRef: RefObject<number>,
  beatsPerBarRef: RefObject<number>,
  method: Method,
  backingTrackSpeed: BackingTrack | undefined,
  tickMetronome: (elapsedMs: number, anchorMs: number, beatsPerBar: number) => void,
  resetMetronomeBeat: () => void,
) {
  const [playing, setPlaying] = useState(false);
  // undefined = overlay masqué ; 0 = "--" (lecture démarrée, count-in audio pas encore
  // commencé) ; 1..N = numéro du temps de count-in affiché.
  const [countIn, setCountIn] = useState<number | undefined>(undefined);

  const updateTimer = useRef<number>(null);

  const stopPlayback = () => {
    setPlaying(false);
    audioElmt.current?.pause();
  };

  // Remet la lecture au tout début : audio, curseur AlphaTab et overlay de count-in.
  // Utilisé par le bouton "Début" et à chaque changement de backing track (le simple
  // changement de `src` sur <audio> devrait déjà remettre currentTime à 0, mais on le
  // force explicitement pour ne dépendre d'aucune subtilité de timing du navigateur).
  const resetPlayback = () => {
    stopPlayback();
    setCountIn(undefined);
    resetMetronomeBeat();

    const audio = audioElmt.current;
    if (audio) {
      audio.currentTime = 0;
    }

    const player = alphaTabRef.current?.player;
    if (player) {
      (player.output as unknown as synth.IExternalMediaSynthOutput).updatePosition(0);
    }
  };

  const onTogglePlay = () => {
    if (playing) {
      stopPlayback();
    } else {
      setPlaying(true);
      audioElmt.current?.play();
    }
  };

  const onTimeUpdate = () => {
    const audio = audioElmt.current;
    const api = alphaTabRef.current;

    const player = api?.player;

    if (!audio || !player) {
      return;
    }

    if (audio.paused) {
      // Un dernier timeupdate peut arriver juste après la pause (ordre timeupdate/pause
      // non garanti) — se fier à l'état réel de l'audio plutôt qu'à l'ordre des événements.
      setCountIn(undefined);
      return;
    }

    const output = player.output as unknown as synth.IExternalMediaSynthOutput;
    const audioMs = audio.currentTime * 1000;
    // Lu depuis la ref (toujours à jour), pas depuis la closure du composant : l'interval
    // de ce onTimeUpdate est capturé une fois dans onPlay et ne se recrée pas tout seul si
    // la détection se termine en cours de lecture (track jamais joué avant, cf. bug rapporté).
    const leadInMs = leadInMsRef.current;

    if (leadInMs === undefined) {
      // Pas encore détecté/persisté — comportement d'avant : position brute. On affiche
      // quand même "--" (countIn=0) : la lecture a bel et bien commencé (et le métronome
      // tourne déjà), juste sans timing de count-in connu pour l'instant — accent calé sur
      // t=0 faute de mieux, corrigé dès que countInStartMs devient connu.
      tickMetronome(audioMs, 0, beatsPerBarRef.current);
      setCountIn(0);
      // updatePosition (pas tickPosition : son setter déclenche un vrai seek à chaque appel,
      // ce qui interrompt la lecture au lieu de se contenter de déplacer le curseur)
      output.updatePosition(audioMs);
      return;
    }

    const countInDurationMs =
      (method.defaultCountInBars * beatsPerBarRef.current * 60000) / (backingTrackSpeed?.bpm ?? 1);
    const countInStartMs = leadInMs - countInDurationMs;
    // Piloté par la position audio réelle (pas un setInterval séparé) : reste synchronisé
    // avec play/pause (ne tique pas tant que ce onTimeUpdate ne tourne pas) et avec un seek
    // ou un changement de piste (la position change, le prochain temps calculé suit).
    // Ancré sur countInStartMs pour que l'accent tombe sur le premier vrai temps du
    // count-in, pas sur un temps arbitraire décalé par la durée du silence de tête.
    tickMetronome(audioMs, countInStartMs, beatsPerBarRef.current);

    if (audioMs < countInStartMs) {
      // Encore dans le silence de tête : "--" — la lecture (et le métronome) ont bien
      // démarré, le count-in audio n'a juste pas encore commencé.
      setCountIn(0);
      return;
    }

    if (audioMs < leadInMs) {
      // Count-in en cours : affiche le temps courant (1-indexé), curseur figé sur la 1ère note.
      const totalBeats = method.defaultCountInBars * beatsPerBarRef.current;
      const beatsIntoCountIn = Math.floor(
        ((audioMs - countInStartMs) / 60000) * (backingTrackSpeed?.bpm ?? 1),
      );
      setCountIn(Math.min(beatsIntoCountIn + 1, totalBeats));
      return;
    }

    // Count-in terminé : curseur synchronisé sur le vrai début du morceau.
    //
    // updatePosition() est interprété par AlphaTab comme du temps selon le tempo *noté*
    // dans le .gp (aucun syncPoint configuré). On corrige en mettant à l'échelle : le
    // nombre de temps réellement écoulés au tempo du backing track (trackBpm) doit
    // correspondre au même nombre de temps si on les exprimait au tempo noté — d'où le
    // facteur trackBpm/notatedBpm. Vaut 1 quand le backing track est au tempo noté du .gp.
    setCountIn(undefined);
    const scale = (backingTrackSpeed?.bpm ?? notatedBpmRef.current) / notatedBpmRef.current;
    output.updatePosition((audioMs - leadInMs) * scale);
  };

  const onPlay = () => {
    const api = alphaTabRef.current;

    if (!api) {
      return;
    }

    api.play();
    updateTimer.current = window.setInterval(onTimeUpdate, 50);
    // Le métronome ne tique plus que via onTimeUpdate (voir useMetronome) : ce reset
    // garantit juste que le tout premier temps de cette reprise sonne, même si son index
    // coïncide par coïncidence avec le dernier joué avant l'arrêt précédent.
    resetMetronomeBeat();
  };

  const onPause = () => {
    const api = alphaTabRef.current;

    if (api) {
      api.pause();
    }

    if (updateTimer.current) {
      clearInterval(updateTimer.current);
      updateTimer.current = null;
    }

    // Pas de stopMetronome() à appeler : le métronome ne tique plus que depuis
    // onTimeUpdate, qui ne tourne déjà plus une fois l'interval nettoyé ci-dessus.
    setCountIn(undefined);
    // Couvre aussi la fin naturelle de la lecture (onEnded → onPause) : sans ça, "playing"
    // reste bloqué à true et le bouton continue d'afficher "Pause" alors que ça s'est arrêté.
    setPlaying(false);
  };

  const onVolumeChange = () => {
    const audio = audioElmt.current;
    const api = alphaTabRef.current;

    if (!audio || !api) {
      return;
    }

    api.masterVolume = audio.volume;
  };

  const onRateChange = () => {
    const audio = audioElmt.current;
    const api = alphaTabRef.current;

    if (!audio || !api) {
      return;
    }

    api.playbackSpeed = audio.playbackRate;
  };

  return {
    playing,
    countIn,
    resetPlayback,
    onTogglePlay,
    audioProps: { onTimeUpdate, onPlay, onPause, onVolumeChange, onRateChange },
  };
}
