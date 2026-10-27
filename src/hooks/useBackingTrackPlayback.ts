import type { AlphaTabApi, synth } from '@coderline/alphatab';
import { type RefObject, useRef, useState } from 'react';
import type { BackingStatus } from '@/lib/ipc';
import type { BackingTrack, Method } from '@/types/model';
import type { BackingEngine } from './useBackingEngine';

export default function useBackingTrackPlayback(
  alphaTabRef: RefObject<AlphaTabApi | null>,
  engine: BackingEngine,
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
  // Dernier état de lecture vu dans le statut du moteur, pour détecter les transitions
  // lecture ↔ arrêt (l'équivalent des évènements play/pause/ended de l'ancien <audio>).
  const wasPlaying = useRef(false);

  const stopPlayback = () => {
    setPlaying(false);
    engine.pause();
  };

  // Remet la lecture au tout début : audio, curseur AlphaTab et overlay de count-in.
  // Utilisé par le bouton "Début" et à chaque changement de backing track.
  const resetPlayback = () => {
    stopPlayback();
    setCountIn(undefined);
    resetMetronomeBeat();
    engine.seek(0);

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
      engine.play();
    }
  };

  // trackBpm / notatedBpm — cf. le commentaire de la mise à l'échelle en fin de onStatus.
  const tempoScale = () =>
    (backingTrackSpeed?.bpm || notatedBpmRef.current) / notatedBpmRef.current;

  // Position du curseur hors lecture (seek pendant la pause) : dans le silence de tête ou le
  // count-in, curseur sur la 1ère note, comme pendant la lecture.
  const pausedCursorMs = (audioMs: number, leadInMs: number | undefined) =>
    leadInMs === undefined ? audioMs : Math.max(0, (audioMs - leadInMs) * tempoScale());

  const onStarted = () => {
    alphaTabRef.current?.play();
    // Le métronome ne tique que via onStatus (voir useMetronome) : ce reset garantit juste
    // que le tout premier temps de cette reprise sonne, même si son index coïncide avec le
    // dernier joué avant l'arrêt précédent.
    resetMetronomeBeat();
  };

  const onStopped = () => {
    alphaTabRef.current?.pause();
    setCountIn(undefined);
    // Couvre aussi la fin naturelle du track : sans ça, "playing" reste bloqué à true et le
    // bouton continue d'afficher "Pause" alors que ça s'est arrêté.
    setPlaying(false);
  };

  // Transitions lecture ↔ arrêt (l'équivalent des évènements play/pause/ended de l'ancien <audio>).
  const onTransition = (s: BackingStatus) => {
    if (s.playing === wasPlaying.current) {
      return;
    }
    wasPlaying.current = s.playing;
    if (s.playing) {
      onStarted();
      return;
    }
    onStopped();
    if (s.ended) {
      // Fin du track : retour au début, le curseur suit via le statut du seek (branche
      // « pause » de onStatus) au lieu de rester bloqué en fin de tablature.
      engine.seek(0);
    }
  };

  // Appelé à chaque statut du moteur (~60 Hz pendant la lecture, à chaque changement sinon) :
  // le moteur est l'horloge maître, AlphaTab ne fait que suivre.
  const onStatus = (s: BackingStatus) => {
    onTransition(s);

    const player = alphaTabRef.current?.player;
    if (!player) {
      return;
    }

    const output = player.output as unknown as synth.IExternalMediaSynthOutput;
    const audioMs = s.positionMs;
    // Lu depuis la ref (toujours à jour), pas depuis une closure : la détection du silence peut
    // se terminer en cours de lecture (track jamais joué avant).
    const leadInMs = leadInMsRef.current;

    if (!s.playing) {
      // Seek pendant la pause : le curseur suit quand même la nouvelle position, sans
      // métronome ni overlay.
      setCountIn(undefined);
      output.updatePosition(pausedCursorMs(audioMs, leadInMs));
      return;
    }

    if (leadInMs === undefined) {
      // Pas encore détecté/persisté : position brute, "--" affiché (la lecture a bel et bien
      // commencé, sans timing de count-in connu), accent calé sur t=0 faute de mieux.
      tickMetronome(audioMs, 0, beatsPerBarRef.current);
      setCountIn(0);
      // updatePosition (pas tickPosition : son setter déclenche un vrai seek à chaque appel,
      // ce qui interrompt la lecture au lieu de se contenter de déplacer le curseur)
      output.updatePosition(audioMs);
      return;
    }

    const countInDurationMs =
      (method.defaultCountInBars * beatsPerBarRef.current * 60000) /
      (backingTrackSpeed?.bpm || notatedBpmRef.current);
    const countInStartMs = leadInMs - countInDurationMs;
    // Piloté par la position du moteur : reste synchronisé avec play/pause, seek et changement
    // de piste. Ancré sur countInStartMs pour que l'accent tombe sur le premier vrai temps du
    // count-in, pas sur un temps décalé par la durée du silence de tête.
    tickMetronome(audioMs, countInStartMs, beatsPerBarRef.current);

    if (audioMs < countInStartMs) {
      // Encore dans le silence de tête : "--".
      setCountIn(0);
      return;
    }

    if (audioMs < leadInMs) {
      // Count-in en cours : temps courant (1-indexé), curseur figé sur la 1ère note.
      const totalBeats = method.defaultCountInBars * beatsPerBarRef.current;
      const beatsIntoCountIn = Math.floor(
        ((audioMs - countInStartMs) / 60000) * (backingTrackSpeed?.bpm || notatedBpmRef.current),
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
    output.updatePosition((audioMs - leadInMs) * tempoScale());
  };

  engine.onStatus(onStatus);

  return {
    playing,
    countIn,
    resetPlayback,
    onTogglePlay,
  };
}
