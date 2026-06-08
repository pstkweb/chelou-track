// Contract test: the JSON shape that Rust's ManifestStore serializes must satisfy
// the TypeScript interfaces in types/model.ts.
//
// No test runner needed — `tsc --noEmit` (part of `npm run ci`) validates this file.
// If the Rust serde attributes and the TS interfaces diverge, this file will fail to compile.
import type {
  BackingGroup,
  BackingTrack,
  DocumentRef,
  FileRef,
  Lesson,
  Method,
  SyncPoint,
  TabSet,
} from "../types/model";

// Represents a JSON object produced by serde_json::to_string(&method) on the Rust side.
// Field names must be camelCase (enforced by serde rename attributes).
// Optional fields (Option::is_none + skip_serializing_if) must be absent, not null.
export const SAMPLE_METHOD = {
  id: "test-method",
  title: "Test Method",
  source: { provider: "pcloud" as const, rootFolderId: 123456789 },
  defaultCountInBars: 1,
  lessons: [
    {
      id: "lesson-1",
      order: 0,
      title: "Lesson 1",
      videos: [{ fileId: 1, name: "video.mp4" }] satisfies FileRef[],
      tabs: [
        // gpx is absent — Rust serializes None with skip_serializing_if, not null
        { id: "tab-1", title: "Tab 1", gp: { fileId: 2, name: "tab.gp" } },
      ] satisfies TabSet[],
      backingGroups: [
        {
          label: "partie distorsion",
          tracks: [
            // leadInMsOverride and syncPoints absent — same skip_serializing_if rule
            {
              audio: {
                fileId: 3,
                name: "Backing track partie distorsion (120bpm).wav",
              },
              bpm: 120,
            },
          ] satisfies BackingTrack[],
        },
      ] satisfies BackingGroup[],
    },
  ] satisfies Lesson[],
  documents: [
    { file: { fileId: 4, name: "sheet.pdf" }, kind: "pdf" as const, title: "Sheet" },
  ] satisfies DocumentRef[],
} satisfies Method;

// SyncPoint shape (used in leadInMsOverride override paths)
export const SAMPLE_SYNC_POINT = {
  audioMs: 1234.5,
  tick: 3840,
} satisfies SyncPoint;
