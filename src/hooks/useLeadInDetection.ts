import { type RefObject, useEffect, useState } from 'react';
import { useLibrary } from '@/contexts/LibraryContext';
import type { BackingInfo } from '@/lib/ipc';
import { leadInFromSilence } from '@/lib/lead-in';
import type { BackingTrack, Lesson, Method } from '@/types/model';

export default function useLeadInDetection(
  method: Method,
  lesson: Lesson,
  backingTrackSpeed: BackingTrack | undefined,
  // Track chargé par le moteur : porte la fin du silence de tête, détectée côté Rust au
  // décodage (aucun téléchargement supplémentaire ici).
  backingInfo: BackingInfo | undefined,
  scoreLoadedRef: RefObject<Promise<void>>,
  beatsPerBarRef: RefObject<number>,
  notatedBpmRef: RefObject<number>,
) {
  const { setLeadIn } = useLibrary();
  const [detectedLeadInMs, setDetectedLeadInMs] = useState<number | undefined>(undefined);

  // Dès que le moteur a chargé un track sans lead-in en cache, calcule le lead-in à partir du
  // silence détecté au décodage et le persiste — c'est ce qui remplit réellement
  // leadInMsOverride (l'écran de debug ne l'écrit jamais).
  useEffect(() => {
    // Un nouveau track (autre bpm ou autre tab) n'a pas hérité du lead-in du précédent —
    // sans ce reset, le local override du track d'avant resterait actif le temps que la
    // détection du nouveau tourne (ou pire, indéfiniment si le nouveau track a déjà un
    // leadInMsOverride persisté, puisque le guard ci-dessous sort alors avant d'y arriver).
    setDetectedLeadInMs(undefined);

    if (!backingTrackSpeed || backingTrackSpeed.leadInMsOverride !== undefined) {
      return;
    }

    let cancelled = false;
    const track = backingTrackSpeed;

    scoreLoadedRef.current
      .then(() => {
        if (cancelled) {
          return;
        }
        const leadInMs = leadInFromSilence(
          track.audio.fileId,
          backingInfo,
          method.defaultCountInBars,
          beatsPerBarRef.current,
          track.bpm || notatedBpmRef.current,
        );
        if (leadInMs === undefined) {
          return;
        }
        // Utilisable dès maintenant pour cette session (count-in, sync curseur) — la
        // persistance ci-dessous est fire-and-forget et ne fait pas revivre l'état local.
        setDetectedLeadInMs(leadInMs);
        return setLeadIn(method.id, lesson.id, track.audio.fileId, leadInMs);
      })
      .catch((err) => {
        console.warn('lead-in persistence failed', err);
      });

    return () => {
      cancelled = true;
    };
  }, [
    backingTrackSpeed,
    backingInfo,
    method.id,
    method.defaultCountInBars,
    lesson.id,
    beatsPerBarRef,
    notatedBpmRef,
    scoreLoadedRef,
    setLeadIn,
  ]);

  return detectedLeadInMs;
}
