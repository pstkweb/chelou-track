import { Catalogue } from './components/catalogue/Catalogue';
import { VideoPlayer } from './components/player/VideoPlayer';
import { SyncView } from './components/player/SyncView';
import { DocViewer } from './components/documents/DocViewer';
import type { Lesson, Method } from './types/model';

export class App {
  private readonly catalogue: Catalogue;
  private readonly videoPlayer: VideoPlayer;
  private readonly syncView: SyncView;
  private readonly docViewer: DocViewer;

  constructor() {
    this.catalogue   = new Catalogue(document.getElementById('catalogue')!);
    this.videoPlayer = new VideoPlayer(document.getElementById('video-player')!);
    this.syncView    = new SyncView(document.getElementById('sync-view')!);
    this.docViewer   = new DocViewer(document.getElementById('doc-viewer')!);

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
