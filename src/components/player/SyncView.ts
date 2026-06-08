// Audio-master / AlphaTab-slave sync view — cf. ARCHITECTURE.md §7.
//
// AlphaTab 1.8.3 (verified against installed @coderline/alphatab@1.8.3):
//  - PlayerMode.EnabledExternalMedia = 4
//  - IExternalMediaHandler: AlphaTab calls play/pause/seekTo on us; we push ticks in rAF
//  - api.player.output cast to IExternalMediaSynthOutput to set the handler
//  - api.tickPosition = tick  ← drives the cursor (bypasses score BPM, uses backing BPM)
//  - Do NOT use output.updatePosition(ms): would convert with score BPM, wrong for slow versions
import type {
  IExternalMediaHandler,
  IExternalMediaSynthOutput,
} from '@coderline/alphatab';
import * as alphaTab from '@coderline/alphatab';

import { audioUrl, tabUrl } from '../../lib/stream';
import type { BackingTrack, TabSet } from '../../types/model';

// Standard MIDI ticks per quarter note used by AlphaTab.
const TICKS_PER_BEAT = 960;

// ---------------------------------------------------------------------------
// AudioHandler — implements IExternalMediaHandler
// AlphaTab calls play/pause/seekTo when its own UI triggers transport events.
// We also use it to expose the AudioContext time for the rAF loop.
// ---------------------------------------------------------------------------
class AudioHandler implements IExternalMediaHandler {
  private audioCtx: AudioContext;
  private buffer: AudioBuffer;
  private sourceNode: AudioBufferSourceNode | null = null;
  /** Wall-clock AudioContext.currentTime at which WAV offset 0 was playing. */
  t0 = 0;
  private playOffset = 0; // seconds into the buffer when we last started

  constructor(audioCtx: AudioContext, buffer: AudioBuffer) {
    this.audioCtx = audioCtx;
    this.buffer = buffer;
  }

  get backingTrackDuration(): number {
    return this.buffer.duration * 1000;
  }

  get playbackRate(): number { return this.sourceNode?.playbackRate.value ?? 1; }
  set playbackRate(v: number) { if (this.sourceNode) this.sourceNode.playbackRate.value = v; }

  get masterVolume(): number { return 1; /* TODO: wire GainNode */ }
  set masterVolume(_v: number) { /* TODO: wire GainNode */ }

  play(): void {
    this._startFrom(this.playOffset);
  }

  pause(): void {
    this.playOffset = this.currentPositionSeconds();
    this._stopSource();
  }

  seekTo(timeMs: number): void {
    this.playOffset = timeMs / 1000;
    if (this.sourceNode) this._startFrom(this.playOffset);
  }

  /** Elapsed seconds in the WAV since the handler started. */
  currentPositionSeconds(): number {
    if (!this.sourceNode) return this.playOffset;
    return this.audioCtx.currentTime - this.t0;
  }

  private _startFrom(offsetSeconds: number): void {
    this._stopSource();
    const node = this.audioCtx.createBufferSource();
    node.buffer = this.buffer;
    node.connect(this.audioCtx.destination);
    const safeOffset = Math.max(0, Math.min(offsetSeconds, this.buffer.duration));
    this.t0 = this.audioCtx.currentTime - safeOffset;
    node.start(0, safeOffset);
    this.sourceNode = node;
  }

  private _stopSource(): void {
    try { this.sourceNode?.stop(); } catch { /* already stopped */ }
    this.sourceNode = null;
  }
}

// ---------------------------------------------------------------------------
// SyncView
// ---------------------------------------------------------------------------
export class SyncView {
  private api: alphaTab.AlphaTabApi | null = null;
  private handler: AudioHandler | null = null;
  private audioCtx: AudioContext | null = null;

  private leadInMs = 0;
  private trackBpm = 120;
  private beatsPerBar = 4; // updated from score after load

  private rafId: number | null = null;

  constructor(private readonly container: HTMLElement) {}

  /** Load a GuitarPro file into AlphaTab. Call once per lesson. */
  async loadTab(tab: TabSet): Promise<void> {
    const fileId = tab.gp?.fileId ?? tab.gpx?.fileId;
    if (fileId === undefined) throw new Error('TabSet has no .gp / .gpx');

    this.stop();
    this.api?.destroy();

    this.container.innerHTML = '';
    const el = document.createElement('div');
    this.container.appendChild(el);

    this.api = new alphaTab.AlphaTabApi(el, {
      player: {
        // PlayerMode.EnabledExternalMedia = 4 (verified in @coderline/alphatab@1.8.3)
        playerMode: alphaTab.PlayerMode.EnabledExternalMedia,
      },
    });

    // Wire the handler once the player is ready.
    this.api.playerReady.on(() => {
      const player = this.api!.player;
      if (player && this.handler) {
        (player.output as IExternalMediaSynthOutput).handler = this.handler;
      }
    });

    // Capture beatsPerBar from the loaded score for automatic leadInMs.
    this.api.scoreLoaded.on((score: alphaTab.model.Score) => {
      const firstBar = score.masterBars[0];
      if (firstBar) this.beatsPerBar = firstBar.timeSignatureNumerator;
    });

    // Fetch the tab bytes and hand them to AlphaTab.
    const resp = await fetch(tabUrl(fileId));
    const bytes = await resp.arrayBuffer();
    this.api.loadArray(new Uint8Array(bytes));
  }

  /** Load a backing WAV. Call after loadTab, before play(). */
  async loadBacking(track: BackingTrack, defaultCountInBars: number): Promise<void> {
    this.trackBpm = track.bpm;

    const resp = await fetch(audioUrl(track.audio.fileId));
    const bytes = await resp.arrayBuffer();

    this.audioCtx ??= new AudioContext();
    const buffer = await this.audioCtx.decodeAudioData(bytes);

    this.handler = new AudioHandler(this.audioCtx, buffer);

    // Wire handler if playerReady already fired.
    const player = this.api?.player;
    if (player) {
      (player.output as IExternalMediaSynthOutput).handler = this.handler;
    }

    // leadInMs = countInBars × beatsPerBar × 60000 / trackBpm  (cf. §7)
    this.leadInMs = track.leadInMsOverride
      ?? (defaultCountInBars * this.beatsPerBar * 60_000 / this.trackBpm);
  }

  /** Start playback. AlphaTab calls handler.play() internally. */
  play(): void {
    if (!this.api || !this.handler) return;
    this.handler.play();
    this.api.play();
    this.startSyncLoop();
  }

  stop(): void {
    this.api?.stop();
    this.handler?.pause();
    if (this.rafId !== null) { cancelAnimationFrame(this.rafId); this.rafId = null; }
  }

  private startSyncLoop(): void {
    const loop = () => {
      if (!this.handler || !this.api) return;
      const audioMs = this.handler.currentPositionSeconds() * 1000;

      // During count-in (audioMs < leadInMs) the cursor stays at tick 0 (§7).
      const beatsElapsed = Math.max(0, (audioMs - this.leadInMs) / 60_000 * this.trackBpm);
      const tick = Math.floor(beatsElapsed * TICKS_PER_BEAT);

      // Drive AlphaTab cursor with backing-track BPM — bypasses score BPM (correct for slow versions).
      this.api.tickPosition = tick;

      this.rafId = requestAnimationFrame(loop);
    };
    this.rafId = requestAnimationFrame(loop);
  }
}
