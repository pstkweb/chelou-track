import type {
  Lesson,
  LessonItem,
  LessonProgress,
  Method,
  Section,
  SectionItem,
} from '@/types/model';

export type LessonStatus = 'todo' | 'current' | 'done';

type LessonStat = {
  lessonId: string;
  status: LessonStatus;
  resumeMs: number | undefined;
};

type ChapterMeta = {
  lessonsStatus: LessonStat[];
  num: number;
  tabsCount: number;
};

export type Chapter = Section & ChapterMeta;

export function getChapters(method: Method): Chapter[] {
  const chapters = [];
  let i = 1;

  for (const item of method.items) {
    if (item.type === 'lesson') {
      continue;
    }

    const chapterMeta: ChapterMeta = { lessonsStatus: [], tabsCount: 0, num: i };
    iterateFolder(item, chapterMeta, method.progress);

    chapters.push({ ...item, ...chapterMeta });
    i++;
  }

  return chapters.sort((a, b) => a.num - b.num);
}

function iterateFolder(
  item: SectionItem,
  stats: ChapterMeta,
  methodProgress: Record<string, LessonProgress>,
) {
  if (item.type === 'lesson') {
    let status: LessonStatus = 'todo';

    if (methodProgress[item.id]) {
      if (methodProgress[item.id]?.resumeMs) {
        status = 'current';
      } else {
        status = 'done';
      }
    }

    stats.lessonsStatus.push({
      lessonId: item.id,
      status,
      resumeMs: methodProgress[item.id]?.resumeMs,
    });
    stats.tabsCount += item.tabs.length;
  } else {
    for (const subItem of item.items) {
      iterateFolder(subItem, stats, methodProgress);
    }
  }
}

export type MethodStats = {
  chaptersCount: number;
  documentsCount: number;
  hasProgress: boolean;
  tabsCount: number;
  videosCount: number;
  videosDone: number;
};

export function computeStats(method: Method): MethodStats {
  const chapters = getChapters(method);
  let lessonsCount = 0;
  let lessonsDoneCount = 0;
  let tabsCount = 0;

  for (const chapter of chapters) {
    lessonsCount += chapter.lessonsStatus.length;
    tabsCount += chapter.tabsCount;
    lessonsDoneCount += chapter.lessonsStatus.filter((stat) => stat.status === 'done').length;
  }

  return {
    chaptersCount: method.items.filter((item) => item.type === 'section').length,
    documentsCount: method.documents.length,
    hasProgress: Object.keys(method.progress).length > 0,
    tabsCount,
    videosCount: lessonsCount,
    videosDone: lessonsDoneCount,
  };
}

export function searchChapter(chapters: Chapter[], lessonId: string) {
  for (const chapter of chapters) {
    if (chapter.lessonsStatus.some((stat) => stat.lessonId === lessonId)) {
      return chapter;
    }
  }

  return undefined;
}

function reduceItems<T>(items: SectionItem[], fn: (acc: T, lesson: LessonItem) => T, init: T): T {
  return items.reduce<T>((acc, item) => {
    if (item.type === 'lesson') {
      return fn(acc, item);
    }

    if (item.type === 'section') {
      return reduceItems(item.items, fn, acc);
    }

    return acc;
  }, init);
}

export const countLessons = (items: SectionItem[]): number =>
  reduceItems(items, (sum) => sum + 1, 0);
export const countTabs = (items: SectionItem[]): number =>
  reduceItems(items, (sum, lesson) => sum + lesson.tabs.length, 0);
export const countBackingTracks = (items: SectionItem[]) =>
  reduceItems(items, (sum, lesson) => sum + lesson.backingGroups.length, 0);

export function searchLesson(chapters: Chapter[], lessonId: string) {
  let currentLesson: { chapter: Chapter; lesson: Lesson } | undefined;

  const chapter = searchChapter(chapters, lessonId);

  if (chapter) {
    const lesson = findLesson(lessonId, chapter.items);

    if (lesson) {
      currentLesson = { chapter, lesson };
    }
  }

  return currentLesson;
}

function findLesson(lessonId: string, items: SectionItem[]): Lesson | undefined {
  for (const item of items) {
    if (item.type === 'lesson' && item.id === lessonId) {
      return item;
    }

    if (item.type === 'section') {
      const found = findLesson(lessonId, item.items);
      if (found) return found;
    }
  }

  return undefined;
}

export function searchSiblings(chapter: Chapter, lessonOrder: number) {
  return findSiblings(lessonOrder, chapter.items);
}

function findSiblings(lessonOrder: number, items: SectionItem[]): [Lesson | null, Lesson | null] {
  let prev = null;
  let next = null;

  for (const item of items) {
    if (item.type === 'lesson') {
      if (item.order === lessonOrder - 1) {
        prev = item;
      } else if (item.order === lessonOrder + 1) {
        next = item;
      }
    }

    if (prev && next) {
      break;
    }

    if (item.type === 'section') {
      const [foundPrev, foundNext] = findSiblings(lessonOrder, item.items);

      prev ??= foundPrev;
      next ??= foundNext;
    }
  }

  return [prev, next];
}

export function getNextLessonId(method: Method, chapters: Chapter[]): string | undefined {
  if (Object.keys(method.progress).length === 0) {
    return chapters[0]?.lessonsStatus[0]?.lessonId;
  }

  for (const chapter of chapters) {
    for (const stat of chapter.lessonsStatus) {
      if (stat.status !== 'done') {
        return stat.lessonId;
      }
    }
  }

  return undefined;
}

/**
 * Returns the fraction of lessons marked seen, in [0, 1].
 * Returns 0 for empty methods.
 */
export function methodProgressPct(method: Method): number {
  const total = countLessons(method.items);

  if (total === 0) {
    return 0;
  }

  return Object.keys(method.progress).length / total;
}
