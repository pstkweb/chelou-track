import type { Lesson, LessonProgress, Method, Section, SectionItem } from "@/types/model";

export type LessonStatus = "todo" | "current" | "done";

type LessonStat = {
  lessonId: string;
  status: LessonStatus;
};

type ChapterMeta = {
  lessonsStatus: LessonStat[];
  num: number;
  tabsCount: number;
};

export type Chapter = Section & ChapterMeta;

export function getChapters(method: Method): Chapter[] {
  const chaptersStats: { [chapterId: string]: ChapterMeta } = {};
  const chapters = [];
  let i = 1;

  for (const item of method.items) {
    if (item.type === "lesson") {
      continue;
    }

    const chapterMeta: ChapterMeta = { lessonsStatus: [], tabsCount: 0, num: i };
    chaptersStats[item.id] = chapterMeta;

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
  if (item.type === "lesson") {
    let status: LessonStatus = "todo";

    if (methodProgress[item.id]) {
      if (methodProgress[item.id]?.resumeMs) {
        status = "current";
      } else {
        status = "done";
      }
    }

    stats.lessonsStatus.push({ lessonId: item.id, status });
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
    lessonsDoneCount += chapter.lessonsStatus.filter((stat) => stat.status === "done").length;
  }

  return {
    chaptersCount: method.items.filter((item) => item.type === "section").length,
    documentsCount: method.documents.length,
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

export function countLessons(items: SectionItem[]): number {
  return items.reduce((n, item) => n + (item.type === "lesson" ? 1 : countLessons(item.items)), 0);
}

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
    if (item.type === "lesson" && item.id === lessonId) {
      return item;
    }

    if (item.type === "section") {
      return findLesson(lessonId, item.items);
    }
  }

  return undefined;
}
