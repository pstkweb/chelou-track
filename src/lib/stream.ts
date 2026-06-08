// Helpers to build stream:// URIs for the Rust protocol handler.
// The backend resolves these to pCloud byte ranges — no URL leaks to the WebView.

export function videoUrl(fileId: number, transcoded = false): string {
  const base = `stream://video/${fileId}`;
  return transcoded ? `${base}?transcoded=true` : base;
}

export function audioUrl(fileId: number): string {
  return `stream://audio/${fileId}`;
}

export function tabUrl(fileId: number): string {
  return `stream://tab/${fileId}`;
}

export function docUrl(fileId: number): string {
  return `stream://doc/${fileId}`;
}
