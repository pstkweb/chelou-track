import { useMemo } from "react";
import { computeStats, getChapters, searchChapter, searchLesson } from "@/lib/method-view";
import type { Method } from "@/types/model";

export default function useMethodView(method: Method) {
  const currentLessonId = Object.keys(method.progress).at(-1);
  const chapters = useMemo(() => getChapters(method), [method]);
  const stats = useMemo(() => computeStats(method), [method]);
  const currentLesson = useMemo(
    () => (currentLessonId ? searchLesson(chapters, currentLessonId) : undefined),
    [chapters, currentLessonId],
  );
  const currentChapter = useMemo(
    () => (currentLessonId ? searchChapter(chapters, currentLessonId) : undefined),
    [chapters, currentLessonId],
  );

  return { chapters, stats, currentChapter, currentLesson };
}
