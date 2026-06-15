// Typed wrappers around Tauri invoke() — the only way the frontend talks to pCloud.
// Never call pCloud APIs directly from TS (cf. ARCHITECTURE.md §3 + §5).
import { invoke } from "@tauri-apps/api/core";
import type { Method } from "../types/model";

// --- Auth ---
// pcloudLogin will be added once the OAuth flow is implemented (client_id pending).

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
