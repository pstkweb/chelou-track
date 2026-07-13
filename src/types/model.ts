// Data model — source of truth: docs/ARCHITECTURE.md §8
// Mirrors the Rust structs in src-tauri/crates/manifest/src/lib.rs.

export interface Method {
  id: string;
  title: string;
  source: { provider: 'pcloud'; rootFolderId: number };
  defaultCountInBars: number;
  /** Ordered mix of lessons and sections at the method root, in DFS natural sort. */
  items: SectionItem[];
  documents: DocumentRef[];
  /**
   * Per-lesson progress keyed by lesson `id`.
   * Presence of an entry means the lesson has been seen at least once.
   */
  progress: Record<string, LessonProgress>;
}

/** Viewing progress for a single lesson. */
export interface LessonProgress {
  /** Playback position in ms where the user stopped mid-video. */
  resumeMs?: number;
}

/**
 * One item inside a folder: either a video lesson or a structural sub-folder.
 * Discriminated by the `type` field ("lesson" | "section").
 */
export type SectionItem = LessonItem | SectionFolder;

export interface LessonItem extends Lesson {
  type: 'lesson';
}

export interface SectionFolder extends Section {
  type: 'section';
}

/** A structural folder (chapter, episode, part…). Arbitrarily nestable. */
export interface Section {
  id: string;
  title: string;
  /** Ordered mix of lessons and sub-sections, interleaved as in the pCloud folder. */
  items: SectionItem[];
  documents: DocumentRef[];
}

export interface Lesson {
  id: string;
  /** Global DFS viewing order across the whole method (1-based). */
  order: number;
  title: string;
  video: FileRef;
  tabs: TabSet[];
  backingGroups: BackingGroup[];
}

export interface TabFile {
  ext: string;
  file: FileRef;
}

export interface TabSet {
  id: string;
  title: string;
  /** Formats disponibles, dans l'ordre de préférence : gp > gpx > gp5 > gp4 > gp3 */
  files: TabFile[];
}

export interface BackingGroup {
  label: string; // e.g. "partie distorsion"
  tracks: BackingTrack[]; // sorted by bpm
}

export interface BackingTrack {
  audio: FileRef;
  bpm: number;
  // Final lead-in offset (ms). If absent, derived from bpm + beatsPerBar + defaultCountInBars.
  // Populated automatically by TabScreen's background detection, or set manually to correct
  // a track — same field either way, whichever set it last wins.
  leadInMsOverride?: number;
}

export interface DocumentRef {
  file: FileRef;
  kind: 'pdf' | 'image';
  title: string;
}

export interface FileRef {
  fileId: number;
  name: string;
}
