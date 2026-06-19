// Data model — source of truth: docs/ARCHITECTURE.md §8
// Mirrors the Rust structs in src-tauri/crates/manifest/src/lib.rs.

export interface Method {
  id: string;
  title: string;
  source: { provider: "pcloud"; rootFolderId: number };
  defaultCountInBars: number;
  /** Ordered mix of lessons and sections at the method root, in DFS natural sort. */
  items: SectionItem[];
  documents: DocumentRef[];
}

/**
 * One item inside a folder: either a video lesson or a structural sub-folder.
 * Discriminated by the `type` field ("lesson" | "section").
 */
export type SectionItem = LessonItem | SectionFolder;

export interface LessonItem extends Lesson {
  type: "lesson";
}

export interface SectionFolder extends Section {
  type: "section";
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
  videos: FileRef[];
  tabs: TabSet[];
  backingGroups: BackingGroup[];
}

export interface TabSet {
  id: string;
  title: string;
  gp?: FileRef; // priority
  gpx?: FileRef; // fallback
}

export interface BackingGroup {
  label: string; // e.g. "partie distorsion"
  tracks: BackingTrack[]; // sorted by bpm
}

export interface BackingTrack {
  audio: FileRef;
  bpm: number;
  leadInMsOverride?: number; // if absent, derived from bpm + beatsPerBar + defaultCountInBars
  syncPoints?: SyncPoint[]; // for exceptional variable-tempo tracks
}

export interface DocumentRef {
  file: FileRef;
  kind: "pdf" | "image";
  title: string;
}

export interface FileRef {
  fileId: number;
  name: string;
}

export interface SyncPoint {
  audioMs: number;
  tick: number;
}
