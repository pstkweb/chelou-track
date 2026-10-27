import type { BackingInfo } from './ipc';

/**
 * `leadInMs = silenceMs + countInBars × beatsPerBar × 60000 / bpm` (ARCHITECTURE.md §7), à
 * partir du silence de tête détecté par le moteur au chargement de `info`. `undefined` si le
 * track n'est pas (encore) chargé, si aucun onset net n'a été trouvé, ou si `info` se rapporte
 * à un autre track que `fileId` — le lead-in étant persisté une fois pour toutes, l'associer au
 * mauvais fichier fausserait son décompte définitivement.
 */
export function leadInFromSilence(
  fileId: string,
  info: BackingInfo | undefined,
  countInBars: number,
  beatsPerBar: number,
  bpm: number,
): number | undefined {
  if (!info || info.fileId !== fileId || info.silenceMs === null) {
    return undefined;
  }
  return info.silenceMs + (countInBars * beatsPerBar * 60000) / bpm;
}
