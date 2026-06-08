// Typed wrappers around Tauri invoke() — the only way the frontend talks to pCloud.
// Never call pCloud APIs directly from TS (cf. ARCHITECTURE.md §3 + §5).
import { invoke } from '@tauri-apps/api/core';
import type { Method } from '../types/model';

// --- Auth ---

export async function pcloudLogin(username: string, password: string): Promise<void> {
  await invoke('pcloud_login', { username, password });
}

export async function pcloudLogout(): Promise<void> {
  await invoke('pcloud_logout');
}

export async function getAuthStatus(): Promise<boolean> {
  return invoke('get_auth_status');
}

// --- Catalogue / manifest ---

export async function listMethods(): Promise<Method[]> {
  return invoke('list_methods');
}

/** Scan a pCloud folder and return a fully-built Method (not yet persisted). */
export async function scanMethod(rootFolderId: number, title: string): Promise<Method> {
  return invoke('scan_method', { rootFolderId, title });
}

export async function saveMethod(method: Method): Promise<void> {
  await invoke('save_method', { method });
}

export async function deleteMethod(id: string): Promise<void> {
  await invoke('delete_method', { id });
}
