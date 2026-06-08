// Video player backed by stream:// — falls back to transcoded (getvideolink) on decode error.
// cf. ARCHITECTURE.md §4 (fallback transcodé) et §11.
import { videoUrl } from "../../lib/stream";
import type { FileRef } from "../../types/model";

export class VideoPlayer {
  private readonly video: HTMLVideoElement;
  private currentFileId: number | null = null;
  private transcoded = false;

  onError: ((fileId: number) => void) | null = null;

  constructor(container: HTMLElement) {
    this.video = document.createElement("video");
    this.video.controls = true;
    this.video.style.width = "100%";
    this.video.addEventListener("error", () => this.handleDecodeError());
    container.appendChild(this.video);
  }

  load(file: FileRef): void {
    this.currentFileId = file.fileId;
    this.transcoded = false;
    this.video.src = videoUrl(file.fileId, false);
    this.video.load();
  }

  private handleDecodeError(): void {
    if (this.currentFileId === null || this.transcoded) return;
    this.transcoded = true;
    this.video.src = videoUrl(this.currentFileId, true);
    this.video.load();
    this.onError?.(this.currentFileId);
  }

  play(): void {
    void this.video.play();
  }
  pause(): void {
    this.video.pause();
  }
  get currentTime(): number {
    return this.video.currentTime;
  }
  set currentTime(t: number) {
    this.video.currentTime = t;
  }
}
