// Local cache for rendered document thumbnails (IndexedDB, keyed by provider+fileId).
// PdfPreview.tsx renders page 1 of a PDF to a canvas just to show a ~400px-wide thumbnail —
// without this cache, that means re-fetching and re-rendering the *entire* PDF from the cloud
// provider every time the documents screen is opened. Best-effort: any failure (private
// browsing, storage quota, IndexedDB unavailable) just falls back to re-rendering, never breaks
// the preview itself.

import type { Provider } from '@/types/model';

const DB_NAME = 'chelou-thumbnails';
const STORE_NAME = 'thumbnails';
const DB_VERSION = 1;

function key(provider: Provider, fileId: string): string {
  return `${provider}:${fileId}`;
}

function openDb(): Promise<IDBDatabase> {
  return new Promise((resolve, reject) => {
    const req = indexedDB.open(DB_NAME, DB_VERSION);
    req.onupgradeneeded = () => {
      req.result.createObjectStore(STORE_NAME);
    };
    req.onsuccess = () => resolve(req.result);
    req.onerror = () => reject(req.error);
  });
}

export async function getCachedThumbnail(
  provider: Provider,
  fileId: string,
): Promise<Blob | undefined> {
  try {
    const db = await openDb();
    return await new Promise((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, 'readonly');
      const req = tx.objectStore(STORE_NAME).get(key(provider, fileId));
      req.onsuccess = () => resolve(req.result as Blob | undefined);
      req.onerror = () => reject(req.error);
    });
  } catch {
    return undefined;
  }
}

export async function setCachedThumbnail(
  provider: Provider,
  fileId: string,
  blob: Blob,
): Promise<void> {
  try {
    const db = await openDb();
    await new Promise<void>((resolve, reject) => {
      const tx = db.transaction(STORE_NAME, 'readwrite');
      tx.objectStore(STORE_NAME).put(blob, key(provider, fileId));
      tx.oncomplete = () => resolve();
      tx.onerror = () => reject(tx.error);
    });
  } catch {
    // Best-effort — a failed write just means we re-render next time.
  }
}
