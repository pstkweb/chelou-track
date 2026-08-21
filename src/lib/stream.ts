// Helpers to build stream:// URIs for the Rust protocol handler.
// The backend resolves these to pCloud byte ranges — no URL leaks to the WebView.

import type { Provider } from '@/types/model';

const BASE = 'http://stream.localhost';

export function videoUrl(provider: Provider, fileId: string, transcoded = false): string {
  const base = `${BASE}/video/${provider}/${fileId}`;

  return transcoded ? `${base}?transcoded=true` : base;
}

export function audioUrl(provider: Provider, fileId: string): string {
  return `${BASE}/audio/${provider}/${fileId}`;
}

export function tabUrl(provider: Provider, fileId: string): string {
  return `${BASE}/tab/${provider}/${fileId}`;
}

export function docUrl(provider: Provider, fileId: string): string {
  return `${BASE}/doc/${provider}/${fileId}`;
}
