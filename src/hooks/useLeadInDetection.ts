import { type RefObject, useEffect, useState } from 'react';
import { useLibrary } from '@/contexts/LibraryContext';
import { detectLeadingSilence } from '@/lib/silence-detection';
import type { BackingTrack, Lesson, Method } from '@/types/model';

export default function useLeadInDetection(
  method: Method,
  lesson: Lesson,
  backingTrackSpeed: BackingTrack | undefined,
  scoreLoadedRef: RefObject<Promise<void>>,
  beatsPerBarRef: RefObject<number>,
  notatedBpmRef: RefObject<number>,
) {
  const { setLeadIn } = useLibrary();
  const [detectedLeadInMs, setDetectedLeadInMs] = useState<number | undefined>(undefined);

  // Runs detection in the background whenever a track without a cached lead-in is
  // selected, and persists the result — this is what actually populates
  // leadInMsOverride for real usage (the debug screen never writes it).
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

    Promise.all([
      detectLeadingSilence(method.source.provider, track.audio.fileId),
      scoreLoadedRef.current,
    ])
      .then(([silenceMs]) => {
        if (cancelled || silenceMs === undefined) {
          return;
        }
        const bpm = track.bpm || notatedBpmRef.current;
        const leadInMs =
          silenceMs + (method.defaultCountInBars * beatsPerBarRef.current * 60000) / bpm;
        // Utilisable dès maintenant pour cette session (count-in, sync curseur) — la
        // persistance ci-dessous est fire-and-forget et ne fait pas revivre l'état local.
        setDetectedLeadInMs(leadInMs);
        return setLeadIn(method.id, lesson.id, track.audio.fileId, leadInMs);
      })
      .catch((err) => {
        console.warn('leading-silence detection/persistence failed', err);
      });

    return () => {
      cancelled = true;
    };
  }, [
    backingTrackSpeed,
    method.id,
    method.defaultCountInBars,
    method.source.provider,
    lesson.id,
    beatsPerBarRef,
    notatedBpmRef,
    scoreLoadedRef,
    setLeadIn,
  ]);

  return detectedLeadInMs;
}
