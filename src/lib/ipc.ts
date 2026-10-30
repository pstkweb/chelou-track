// Typed wrappers around Tauri invoke() — the only way the frontend talks to the Rust backend
// (storage providers, native audio engine). Never call a provider API directly from TS
// (cf. ARCHITECTURE.md §3 + §5).
import { Channel, invoke } from '@tauri-apps/api/core';
import type { Method, Provider } from '@/types/model';

// --- Auth ---

/** Open the pCloud OAuth popup and await completion. Throws on cancel or error. */
export async function oauthStart(provider: Provider): Promise<void> {
  await invoke('oauth_start', { provider });
}

/**
 * Pick a folder (local disk or network mount) in the native dialog and make it the active
 * source — the local provider's equivalent of oauthStart. Resolves false if cancelled.
 */
export async function connectLocal(): Promise<boolean> {
  return invoke('connect_local');
}

export async function logout(): Promise<void> {
  await invoke('logout');
}

export async function getAuthStatus(): Promise<Provider | null> {
  return invoke('get_auth_status');
}

/** Secret for the Linux loopback media server (null on other platforms) — cf. lib/stream.ts. */
export async function getMediaToken(): Promise<string | null> {
  return invoke('get_media_token');
}

// --- folder browsing ---

export interface FolderEntry {
  name: string;
  folderid: string;
}

export interface ScanProgressEvent {
  currentFolder: string;
  foldersVisited: number;
  methodsFound: number;
}

/** Returns only the sub-folder children of the given folder (files are filtered out). */
export async function listFolder(provider: Provider, folderId: string): Promise<FolderEntry[]> {
  const entries: Array<{ id: string; name: string; is_folder: boolean }> = await invoke(
    'list_folder',
    { provider, folderId },
  );

  return entries.filter((e) => e.is_folder).map((e) => ({ name: e.name, folderid: e.id }));
}

// --- Catalogue / manifest ---

export async function listMethods(): Promise<Method[]> {
  return invoke('list_methods');
}

/** Scan a pCloud folder and return a fully-built Method (not yet persisted). */
export async function scanMethod(provider: Provider, rootFolderId: string): Promise<Method[]> {
  return invoke('scan_method', { provider, rootFolderId });
}

export async function saveMethod(method: Method): Promise<void> {
  await invoke('save_method', { method });
}

export async function deleteMethod(id: string): Promise<void> {
  await invoke('delete_method', { id });
}

// --- Progress ---

/** Mark a lesson as seen (idempotent). */
export async function markLessonSeen(methodId: string, lessonId: string): Promise<void> {
  await invoke('mark_lesson_seen', { methodId, lessonId });
}

/** Remove a lesson from the progress map (mark as unseen). */
export async function markLessonUnseen(methodId: string, lessonId: string): Promise<void> {
  await invoke('mark_lesson_unseen', { methodId, lessonId });
}

/**
 * Persist the playback position for a lesson (also marks it as seen).
 * Call this periodically while the video is playing and on pause/stop.
 */
export async function updateLessonResume(
  methodId: string,
  lessonId: string,
  resumeMs: number,
): Promise<void> {
  await invoke('update_lesson_resume', { methodId, lessonId, resumeMs });
}

/**
 * Persist the final computed lead-in (ms) for a backing track into leadInMsOverride.
 * Write-once by convention — callers should only invoke this when
 * `track.leadInMsOverride` is not already set.
 */
export async function updateBackingTrackLeadInOverride(
  methodId: string,
  lessonId: string,
  fileId: string,
  leadInMs: number,
): Promise<void> {
  await invoke('update_backing_track_lead_in_override', { methodId, lessonId, fileId, leadInMs });
}

// --- Backing track (moteur audio natif, cf. ARCHITECTURE.md §7) ---

export interface BackingInfo {
  /** Track auquel se rapportent ces infos (un autre a pu être choisi entre-temps). */
  fileId: string;
  durationMs: number;
  /** Fin du silence de tête, détectée côté Rust ; null si aucun onset net. */
  silenceMs: number | null;
  sampleRate: number;
}

export interface BackingStatus {
  positionMs: number;
  durationMs: number;
  playing: boolean;
  ended: boolean;
  loaded: boolean;
  /** Fréquence de la sortie : si elle change (autre périphérique), le track doit être rechargé. */
  sampleRate: number;
  error: string | null;
}

/** Télécharge, décode et charge entièrement le track dans le moteur (lecture à l'arrêt). */
export async function backingLoad(provider: Provider, fileId: string): Promise<BackingInfo> {
  return invoke('backing_load', { provider, fileId });
}

export async function backingUnload(): Promise<void> {
  await invoke('backing_unload');
}

export async function backingPlay(): Promise<void> {
  await invoke('backing_play');
}

export async function backingPause(): Promise<void> {
  await invoke('backing_pause');
}

export async function backingSeek(ms: number): Promise<void> {
  await invoke('backing_seek', { ms });
}

/** Gain linéaire 0-1. */
export async function backingSetVolume(gain: number): Promise<void> {
  await invoke('backing_set_volume', { gain });
}

/**
 * Reçoit le statut du moteur (~60 Hz pendant la lecture, à chaque changement sinon). Un seul
 * abonnement actif côté Rust : un nouvel appel remplace le précédent. La fonction rendue rend
 * l'abonnement inerte côté WebView.
 */
export async function subscribeBackingStatus(
  onStatus: (status: BackingStatus) => void,
): Promise<() => void> {
  let active = true;
  const channel = new Channel<BackingStatus>();
  channel.onmessage = (status) => {
    if (active) {
      onStatus(status);
    }
  };
  await invoke('backing_status', { channel });
  return () => {
    active = false;
  };
}

// --- Carte son, son direct (cf. ARCHITECTURE.md §7) ---

export type AudioHost = 'asio' | 'wasapi' | 'coreaudio' | 'alsa';

/** Carte ouverte en duplex. Canaux 0-based ; entrée ≠ sortie seulement en WASAPI. */
export interface DuplexConfig {
  host: AudioHost;
  inputDevice: string;
  outputDevice: string;
  inputChannel: number;
  outputPair: number;
  sampleRate: number;
  buffer: number;
}

export interface AudioSettings {
  schemaVersion: number;
  /** null : jamais configuré, sortie seule. */
  duplex: DuplexConfig | null;
  /** false : carte arrêtée par l'utilisateur. */
  engineEnabled: boolean;
  /** 0-100, 80 = 0 dB. */
  guitarFader: number;
  guitarMuted: boolean;
}

export interface DeviceInfo {
  name: string;
  inputs: number;
  outputs: number;
  sampleRates: number[];
  bufferMin: number | null;
  bufferMax: number | null;
}

export interface HostDevices {
  host: AudioHost;
  devices: DeviceInfo[];
}

export interface AudioStatus {
  state: 'off' | 'running' | 'stopped' | 'error';
  error: string | null;
  sampleRate: number;
  /** null hors duplex et en WASAPI (buffer système). */
  buffer: number | null;
  latencyMs: number | null;
  xruns: number;
  /** Crêtes 0-1 depuis le statut précédent. */
  inputLevel: number;
  outputLevel: number;
  guitarFader: number;
  guitarMuted: boolean;
  deviceName: string | null;
}

/** Cartes de chaque pilote (lent en ASIO : chaque pilote est chargé tour à tour). */
export async function audioDevices(): Promise<HostDevices[]> {
  return invoke('audio_devices');
}

export async function audioSettings(): Promise<AudioSettings> {
  return invoke('audio_settings');
}

/** Ouvre et mémorise la carte (null : sortie seule). Un échec d'ouverture arrive par le statut. */
export async function audioConfigure(duplex: DuplexConfig | null): Promise<void> {
  await invoke('audio_configure', { duplex });
}

/** Arrêter (false) / Démarrer, Redémarrer (true) la carte. */
export async function audioSetEnabled(enabled: boolean): Promise<void> {
  await invoke('audio_set_enabled', { enabled });
}

export async function guitarSetFader(fader: number): Promise<void> {
  await invoke('guitar_set_fader', { fader });
}

export async function guitarSetMuted(muted: boolean): Promise<void> {
  await invoke('guitar_set_muted', { muted });
}

export async function audioResetXruns(): Promise<void> {
  await invoke('audio_reset_xruns');
}

/** Statut de la carte toutes les 33 ms, en permanence. Même mécanique que subscribeBackingStatus. */
export async function subscribeAudioStatus(
  onStatus: (status: AudioStatus) => void,
): Promise<() => void> {
  let active = true;
  const channel = new Channel<AudioStatus>();
  channel.onmessage = (status) => {
    if (active) {
      onStatus(status);
    }
  };
  await invoke('audio_status', { channel });
  return () => {
    active = false;
  };
}
