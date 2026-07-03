import { Check } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import ChapterProgress from '@/components/molecules/ChapterProgress';
import cn from '@/lib/cn';
import type { Chapter } from '@/lib/method-view';
import SectionNodes from './SectionNodes';

type SidebarNavProps = {
  currentChapterId: string | undefined;
  chapters: Chapter[];
};

export default function SidebarNav({ chapters, currentChapterId }: SidebarNavProps) {
  const [selectedChapterId, setSelectedChapterId] = useState(
    currentChapterId ?? chapters.at(0)?.id,
  );
  const userPickedRef = useRef(false);

  useEffect(() => {
    if (!userPickedRef.current && currentChapterId) {
      setSelectedChapterId(currentChapterId);
    }
  }, [currentChapterId]);

  const selectChapter = (id: string) => {
    userPickedRef.current = true;
    setSelectedChapterId(id);
  };
  const selectedChapter = chapters.find((chapter) => chapter.id === selectedChapterId);

  if (selectedChapterId === undefined) {
    return (
      <div className="card flex min-h-120 items-center justify-center overflow-hidden">
        <span className="text-fg2 text-xl">Aucun chapitre dans cette méthode</span>
      </div>
    );
  }

  return (
    <div className="card grid min-h-120 grid-cols-[320px_1fr] overflow-clip">
      <div className="scroll sticky top-0 max-h-[calc(100vh-38px)] self-start border-border border-r bg-bg2 p-2">
        {chapters.map((chapter) => {
          const lessonsCount = chapter.lessonsStatus.length;
          const done = chapter.lessonsStatus.filter((stat) => stat.status === 'done').length;
          const allDone = done === lessonsCount;

          return (
            <button
              type="button"
              key={chapter.id}
              onClick={() => selectChapter(chapter.id)}
              className={cn(
                'row-btn h-auto min-h-14',
                chapter.id === selectedChapterId && 'active',
              )}
            >
              <div className={cn('display w-7 text-base text-fg2', allDone && 'text-done')}>
                {allDone ? <Check size={18} /> : String(chapter.num).padStart(2, '0')}
              </div>
              <div className="min-w-0 flex-1">
                <div className="min-w-0 flex-1 overflow-hidden truncate whitespace-normal font-semibold text-sm">
                  {chapter.title}
                </div>
                <div className="mt-2 flex items-center gap-2">
                  <div className="h-1 flex-1 overflow-hidden rounded-full bg-chip">
                    <i
                      className="block h-full rounded-full bg-[linear-gradient(90deg,var(--accent),var(--accent-2))]"
                      style={{ width: `${(done / lessonsCount) * 100}%` }}
                    />
                  </div>
                  <ChapterProgress chapter={chapter} />
                </div>
              </div>
            </button>
          );
        })}
      </div>
      {selectedChapter && (
        <div className="scroll p-(--pad)">
          <div className="eyebrow mb-1">
            Chapitre {selectedChapter.num} · {selectedChapter.lessonsStatus.length} leçon
            {selectedChapter.lessonsStatus.length > 1 ? 's' : ''}
          </div>
          <h3 className="display m-0 mb-4 text-2xl">{selectedChapter.title}</h3>
          <SectionNodes
            items={selectedChapter.items}
            chapter={selectedChapter}
            currentChapterId={currentChapterId}
          />
        </div>
      )}
    </div>
  );
}
