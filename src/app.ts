import { Catalogue } from "./components/catalogue/Catalogue";
import { DocViewer } from "./components/documents/DocViewer";
import { SyncView } from "./components/player/SyncView";
import { VideoPlayer } from "./components/player/VideoPlayer";
import type { Lesson, Method } from "./types/model";

function getElement(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`Element #${id} not found`);
  return el;
}

export class App {
  private readonly catalogue: Catalogue;
  private readonly videoPlayer: VideoPlayer;
  private readonly syncView: SyncView;
  // @ts-expect-error TS6133 -- wired in #36 (DocViewer)
  // biome-ignore lint/correctness/noUnusedPrivateClassMembers: wired in #36
  private readonly docViewer: DocViewer;

  constructor() {
    this.catalogue = new Catalogue(getElement("catalogue"));
    this.videoPlayer = new VideoPlayer(getElement("video-player"));
    this.syncView = new SyncView(getElement("sync-view"));
    this.docViewer = new DocViewer(getElement("doc-viewer"));

    this.catalogue.onLessonSelect = (lesson, method) => this.openLesson(lesson, method);
  }

  async init(): Promise<void> {
    // TODO: check auth status; show login form if unauthenticated
    await this.catalogue.load();
  }

  private openLesson(lesson: Lesson, method: Method): void {
    this.syncView.stop();

    if (lesson.videos[0]) {
      this.videoPlayer.load(lesson.videos[0]);
    }

    // TODO: populate tab selector (lesson.tabs)
    // TODO: populate backing group selector (lesson.backingGroups)
    // Tab + backing selection is a runtime action — the user picks from the pool (cf. §6)
    void method;
  }
}
