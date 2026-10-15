// Helpers to build stream:// URIs for the Rust protocol handler.
// The backend resolves these to pCloud byte ranges — no URL leaks to the WebView.

import { platform } from '@tauri-apps/plugin-os';
import { getMediaToken } from '@/lib/ipc';
import type { Provider } from '@/types/model';

function base(): string {
  const p = platform();
  return p === 'windows' || p === 'android' ? 'http://stream.localhost' : 'stream://localhost';
}

const MEDIA_PORT = 47812;

// Per-session secret required by the Linux loopback media server (cf. media_server.rs): without
// it, any web page open in the user's browser could stream their cloud files from 127.0.0.1.
let mediaToken: string | null = null;

/** Fetch the media server token once, before the first render (see main.tsx). */
export async function initMediaToken(): Promise<void> {
  if (platform() === 'linux') {
    mediaToken = await getMediaToken();
  }
}

function mediaUrl(
  kind: 'video' | 'audio',
  provider: Provider,
  fileId: string,
  transcoded: boolean,
) {
  if (platform() !== 'linux') {
    const url = `${base()}/${kind}/${provider}/${fileId}`;

    return transcoded ? `${url}?transcoded=true` : url;
  }

  const query = new URLSearchParams({ t: mediaToken ?? '' });

  if (transcoded) {
    query.set('transcoded', 'true');
  }

  return `http://127.0.0.1:${MEDIA_PORT}/${kind}/${provider}/${fileId}?${query}`;
}

export function videoUrl(provider: Provider, fileId: string, transcoded = false): string {
  return mediaUrl('video', provider, fileId, transcoded);
}

export function audioUrl(provider: Provider, fileId: string): string {
  return mediaUrl('audio', provider, fileId, false);
}

/**
 * fetch() a stream URL, rejecting on a non-2xx status: Rust answers failures with a 500 and a
 * text body, which a bare fetch() resolves as if it were the file (then cached, decoded…).
 */
export async function fetchStream(url: string): Promise<Response> {
  const res = await fetch(url);

  if (!res.ok) {
    throw new Error(`${res.status} ${await res.text()}`);
  }

  return res;
}

export function tabUrl(provider: Provider, fileId: string): string {
  return `${base()}/tab/${provider}/${fileId}`;
}

export function docUrl(provider: Provider, fileId: string): string {
  return `${base()}/doc/${provider}/${fileId}`;
}
