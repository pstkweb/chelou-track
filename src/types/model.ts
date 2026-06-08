// Data model — source of truth: docs/ARCHITECTURE.md §8
// Mirrors the Rust structs in src-tauri/src/manifest/mod.rs (serde snake_case → camelCase).

export interface Method {
  id: string;
  title: string;
  source: { provider: "pcloud"; rootFolderId: number };
  defaultCountInBars: number;
  lessons: Lesson[];
  documents: DocumentRef[];
}

export interface Lesson {
  id: string;
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
