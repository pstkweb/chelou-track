// Helpers to build stream:// URIs for the Rust protocol handler.
// The backend resolves these to pCloud byte ranges — no URL leaks to the WebView.

const BASE = 'http://stream.localhost';

export function videoUrl(fileId: number, transcoded = false): string {
  const base = `${BASE}/video/${fileId}`;

  return transcoded ? `${base}?transcoded=true` : base;
}

export function audioUrl(fileId: number): string {
  return `${BASE}/audio/${fileId}`;
}

export function tabUrl(fileId: number): string {
  return `${BASE}/tab/${fileId}`;
}

export function docUrl(fileId: number): string {
  return `${BASE}/doc/${fileId}`;
}
