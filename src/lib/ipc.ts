// Typed wrappers around Tauri invoke() — the only way the frontend talks to pCloud.
// Never call pCloud APIs directly from TS (cf. ARCHITECTURE.md §3 + §5).
import { invoke } from '@tauri-apps/api/core';
import type { Method, Provider } from '@/types/model';

// --- Auth ---

/** Open the pCloud OAuth popup and await completion. Throws on cancel or error. */
export async function oauthStart(provider: Provider): Promise<void> {
  await invoke('oauth_start', { provider });
}

export async function logout(): Promise<void> {
  await invoke('logout');
}

export async function getAuthStatus(): Promise<Provider | null> {
  return invoke('get_auth_status');
}

// --- pCloud folder browsing ---

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
  const result: { contents: Array<{ id: string; name: string; is_folder: boolean }> } =
    await invoke('list_folder', { provider, folderId });

  return result.contents.filter((e) => e.is_folder).map((e) => ({ name: e.name, folderid: e.id }));
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
