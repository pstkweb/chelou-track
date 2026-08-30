import { createContext, type Dispatch, useCallback, useContext, useMemo, useReducer } from 'react';
import {
  deleteMethod,
  listMethods,
  markLessonSeen,
  markLessonUnseen,
  saveMethod,
  updateBackingTrackLeadInOverride,
  updateLessonResume,
} from '@/lib/ipc';
import type { LessonProgress, Method, SectionItem } from '@/types/model';

interface LibraryContextValue {
  methods: Method[];
  dispatch: Dispatch<LibraryAction>;
  remove: (method: Method) => Promise<void>;
  add: (method: Method) => Promise<void>;
  refresh: () => Promise<void>;
  /** Marks a lesson as seen with no resume position (watched to completion). */
  markSeen: (methodId: string, lessonId: string) => Promise<void>;
  /** Removes a lesson's progress entry entirely. */
  markUnseen: (methodId: string, lessonId: string) => Promise<void>;
  /** Persists the mid-video resume position for a lesson (also marks it as seen). */
  setResume: (methodId: string, lessonId: string, resumeMs: number) => Promise<void>;
  /** Persists the computed lead-in (ms) for a backing track. */
  setLeadIn: (
    methodId: string,
    lessonId: string,
    fileId: string,
    leadInMs: number,
  ) => Promise<void>;
}

type LibraryAction =
  | { type: 'add'; method: Method }
  | { type: 'delete'; method: Method }
  | { type: 'set'; methods: Method[] }
  | { type: 'setLessonProgress'; methodId: string; lessonId: string; resumeMs: number | undefined }
  | { type: 'clearLessonProgress'; methodId: string; lessonId: string }
  | { type: 'setLeadIn'; methodId: string; lessonId: string; fileId: string; leadInMs: number };

const LibraryContext = createContext<LibraryContextValue | undefined>(undefined);

/** Finds the lesson by id anywhere in the (possibly nested) items tree and applies `update`
 * to its backing tracks — mirrors Rust's `find_lesson_mut` (crates/manifest/src/lib.rs). */
function updateLeadInInItems(
  items: SectionItem[],
  lessonId: string,
  fileId: string,
  leadInMs: number,
): SectionItem[] {
  return items.map((item) => {
    if (item.type === 'lesson') {
      if (item.id !== lessonId) return item;

      return {
        ...item,
        backingGroups: item.backingGroups.map((group) => ({
          ...group,
          tracks: group.tracks.map((track) =>
            track.audio.fileId === fileId ? { ...track, leadInMsOverride: leadInMs } : track,
          ),
        })),
      };
    }

    return { ...item, items: updateLeadInInItems(item.items, lessonId, fileId, leadInMs) };
  });
}

function libraryReducer(library: Method[], action: LibraryAction) {
  switch (action.type) {
    case 'add':
      return library.some((m) => m.id === action.method.id) ? library : [...library, action.method];
    case 'delete':
      return library.filter((m) => m.id !== action.method.id);
    case 'set':
      return action.methods;
    case 'setLessonProgress': {
      const progress: LessonProgress =
        action.resumeMs === undefined ? {} : { resumeMs: action.resumeMs };

      return library.map((m) =>
        m.id === action.methodId
          ? { ...m, progress: { ...m.progress, [action.lessonId]: progress } }
          : m,
      );
    }
    case 'clearLessonProgress':
      return library.map((m) => {
        if (m.id !== action.methodId) return m;

        const progress = { ...m.progress };
        delete progress[action.lessonId];

        return { ...m, progress };
      });
    case 'setLeadIn':
      return library.map((m) =>
        m.id === action.methodId
          ? {
              ...m,
              items: updateLeadInInItems(m.items, action.lessonId, action.fileId, action.leadInMs),
            }
          : m,
      );
  }
}

export function LibraryProvider({ children }: { children: React.ReactNode }) {
  const [methods, dispatch] = useReducer(libraryReducer, []);

  const remove = useCallback(async (method: Method) => {
    await deleteMethod(method.id);

    dispatch({ type: 'delete', method });
  }, []);

  const add = useCallback(async (method: Method) => {
    await saveMethod(method);

    dispatch({ type: 'add', method });
  }, []);

  const refresh = useCallback(async () => {
    const methods = await listMethods();

    dispatch({ type: 'set', methods });
  }, []);

  const markSeen = useCallback(async (methodId: string, lessonId: string) => {
    await markLessonSeen(methodId, lessonId);

    dispatch({ type: 'setLessonProgress', methodId, lessonId, resumeMs: undefined });
  }, []);

  const markUnseen = useCallback(async (methodId: string, lessonId: string) => {
    await markLessonUnseen(methodId, lessonId);

    dispatch({ type: 'clearLessonProgress', methodId, lessonId });
  }, []);

  const setResume = useCallback(async (methodId: string, lessonId: string, resumeMs: number) => {
    await updateLessonResume(methodId, lessonId, resumeMs);

    dispatch({ type: 'setLessonProgress', methodId, lessonId, resumeMs });
  }, []);

  const setLeadIn = useCallback(
    async (methodId: string, lessonId: string, fileId: string, leadInMs: number) => {
      await updateBackingTrackLeadInOverride(methodId, lessonId, fileId, leadInMs);

      dispatch({ type: 'setLeadIn', methodId, lessonId, fileId, leadInMs });
    },
    [],
  );

  const contextValue = useMemo(
    () => ({ methods, dispatch, remove, add, refresh, markSeen, markUnseen, setResume, setLeadIn }),
    [methods, remove, add, refresh, markSeen, markUnseen, setResume, setLeadIn],
  );

  return <LibraryContext.Provider value={contextValue}>{children}</LibraryContext.Provider>;
}

export function useLibrary() {
  const context = useContext(LibraryContext);

  if (context === undefined) {
    throw new Error('useLibrary must be used within a LibraryProvider');
  }

  return context;
}
