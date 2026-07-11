import { ChevronRight, Music, Volume2 } from 'lucide-react';
import Chip from '@/components/atoms/Chip';
import StatusDot from '@/components/atoms/StatusDot';
import { useNavigation } from '@/contexts/NavigationContext';
import cn from '@/lib/cn';
import type { Chapter, LessonStatus } from '@/lib/method-view';
import type { Lesson } from '@/types/model';

type LessonRowProps = {
  active: boolean;
  chapter: Chapter;
  lesson: Lesson;
  status: LessonStatus;
};

export default function LessonRow({ lesson, status, chapter, active }: LessonRowProps) {
  const { openLesson } = useNavigation();

  const handleEnterPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      openLesson(lesson, chapter);
    }
  };

  return (
    <div
      className={cn(`tree-row ${status}`, active && 'active')}
      onClick={() => openLesson(lesson, chapter)}
      onKeyDown={handleEnterPress}
    >
      <div className="relative z-10 flex flex-1 items-center gap-3">
        <StatusDot status={status} />
        <div className="min-w-0 flex-1">
          <div className="t-title">{lesson.title}</div>
          <div className="mt-1 flex flex-wrap items-center gap-3">
            {lesson.tabs.length > 0 && (
              <span className="t-meta inline-flex items-center gap-1 text-accent!">
                <Music size={13} /> Tablature{lesson.tabs.length === 1 ? '' : 's'}
              </span>
            )}
            {lesson.backingGroups.length > 0 && (
              <span className="t-meta inline-flex items-center gap-1">
                <Volume2 size={13} /> {lesson.backingGroups.length} backing track
                {lesson.backingGroups.length > 1 ? 's' : ''}
              </span>
            )}
          </div>
        </div>
        {status === 'current' && <Chip className="h-6 text-accent text-xs">En cours</Chip>}
        <ChevronRight size={16} className="text-fg3" />
      </div>
    </div>
  );
}
