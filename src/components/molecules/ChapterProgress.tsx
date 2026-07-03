import type { Chapter } from '@/lib/method-view';

type ChapterProgressProps = {
  chapter: Chapter;
};

export default function ChapterProgress({ chapter }: ChapterProgressProps) {
  const lessonsCount = chapter.lessonsStatus.length;
  const done = chapter.lessonsStatus.filter((stat) => stat.status === 'done').length;

  return (
    <span className="mono text-fg3 text-xs">
      {done}/{lessonsCount}
    </span>
  );
}
