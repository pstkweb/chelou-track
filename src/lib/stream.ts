// Helpers to build stream:// URIs for the Rust protocol handler.
// The backend resolves these to pCloud byte ranges — no URL leaks to the WebView.

import { platform } from '@tauri-apps/plugin-os';
import type { Provider } from '@/types/model';

function base(): string {
  const p = platform();
  return p === 'windows' || p === 'android' ? 'http://stream.localhost' : 'stream://localhost';
}

const MEDIA_PORT = 47812;

function mediaBase(): string {
  return platform() === 'linux' ? `http://127.0.0.1:${MEDIA_PORT}` : base();
}

export function videoUrl(provider: Provider, fileId: string, transcoded = false): string {
  const url = `${mediaBase()}/video/${provider}/${fileId}`;

  return transcoded ? `${url}?transcoded=true` : url;
}

export function audioUrl(provider: Provider, fileId: string): string {
  return `${mediaBase()}/audio/${provider}/${fileId}`;
}

export function tabUrl(provider: Provider, fileId: string): string {
  return `${base()}/tab/${provider}/${fileId}`;
}

export function docUrl(provider: Provider, fileId: string): string {
  return `${base()}/doc/${provider}/${fileId}`;
}
