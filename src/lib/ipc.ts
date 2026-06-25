// Typed wrappers around Tauri invoke() — the only way the frontend talks to pCloud.
// Never call pCloud APIs directly from TS (cf. ARCHITECTURE.md §3 + §5).
import { invoke } from "@tauri-apps/api/core";
import type { Method } from "../types/model";

// --- Auth ---

/** Open the pCloud OAuth popup and await completion. Throws on cancel or error. */
export async function pcloudOauthStart(): Promise<void> {
  await invoke("pcloud_oauth_start");
}

export async function pcloudLogout(): Promise<void> {
  await invoke("pcloud_logout");
}

export async function getAuthStatus(): Promise<boolean> {
  return invoke("get_auth_status");
}

// --- pCloud folder browsing ---

export interface FolderEntry {
  name: string;
  folderid: number;
}

export interface ScanProgressEvent {
  currentFolder: string;
  foldersVisited: number;
  methodsFound: number;
}

/** Returns only the sub-folder children of the given folder (files are filtered out). */
export async function listFolder(folderId: number): Promise<FolderEntry[]> {
  const result: { contents: Array<{ name: string; isfolder: boolean; folderid?: number }> } =
    await invoke("list_folder", { folderId });
  return result.contents
    .filter((e) => e.isfolder && e.folderid != null)
    .map((e) => ({ name: e.name, folderid: e.folderid ?? 0 }));
}

// --- Catalogue / manifest ---

export async function listMethods(): Promise<Method[]> {
  return invoke("list_methods");
}

/** Scan a pCloud folder and return a fully-built Method (not yet persisted). */
export async function scanMethod(rootFolderId: number): Promise<Method[]> {
  return invoke("scan_method", { rootFolderId });
}

export async function saveMethod(method: Method): Promise<void> {
  await invoke("save_method", { method });
}

export async function deleteMethod(id: string): Promise<void> {
  await invoke("delete_method", { id });
}

// --- Progress ---

/** Mark a lesson as seen (idempotent). */
export async function markLessonSeen(methodId: string, lessonId: string): Promise<void> {
  await invoke("mark_lesson_seen", { methodId, lessonId });
}

/** Remove a lesson from the progress map (mark as unseen). */
export async function markLessonUnseen(methodId: string, lessonId: string): Promise<void> {
  await invoke("mark_lesson_unseen", { methodId, lessonId });
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
  await invoke("update_lesson_resume", { methodId, lessonId, resumeMs });
}

// --- Progress helpers ---

function countLessonsInItems(items: Method["items"]): number {
  return items.reduce((n, item) => {
    if (item.type === "lesson") return n + 1;
    return n + countLessonsInItems(item.items);
  }, 0);
}

/**
 * Returns the fraction of lessons marked seen, in [0, 1].
 * Returns 0 for empty methods.
 */
export function progressPct(method: Method): number {
  const total = countLessonsInItems(method.items);
  if (total === 0) return 0;
  return Object.keys(method.progress).length / total;
}
