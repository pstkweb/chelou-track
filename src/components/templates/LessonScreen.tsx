import { Check, ChevronLeft, ChevronRight, Music, Video } from 'lucide-react';
import { useState } from 'react';
import { useLibrary } from '@/contexts/LibraryContext';
import { useNavigation } from '@/contexts/NavigationContext';
import { type Chapter, searchSiblings } from '@/lib/method-view';
import type { Lesson, Method } from '@/types/model';
import Button from '../atoms/Button';
import IconButton from '../atoms/IconButton';
import NavLesson from '../molecules/NavLesson';
import VideoPlayer from '../organisms/VideoPlayer';

type LessonScreenProps = {
  chapter: Chapter;
  method: Method;
  lesson: Lesson;
  onVideoEnd: () => void;
};

export default function LessonScreen({ lesson, chapter, method, onVideoEnd }: LessonScreenProps) {
  const lessonMeta = chapter.lessonsStatus.find((stat) => stat.lessonId === lesson.id);
  const lessonStatus = lessonMeta?.status || 'todo';

  const { openLesson, openTab, goToMethod } = useNavigation();
  const { markSeen, markUnseen } = useLibrary();
  const [done, setDone] = useState(lessonStatus === 'done');
  const [previousLesson, nextLesson] = searchSiblings(chapter, lesson.order);
  const handleVideoEnd = () => {
    setDone(true);
    onVideoEnd();
  };

  const videoPlayer = (
    <VideoPlayer
      chapterNum={chapter.num}
      fileId={lesson.video.fileId}
      lessonId={lesson.id}
      lessonStatus={lessonStatus}
      methodId={method.id}
      methodTitle={method.title}
      provider={method.source.provider}
      resumeMs={lessonMeta?.resumeMs}
      onVideoEnd={handleVideoEnd}
    />
  );

  const handleToggleDone = () => {
    setDone((d) => {
      if (!d) {
        markSeen(method.id, lesson.id);
      } else {
        markUnseen(method.id, lesson.id);
      }

      return !d;
    });
  };

  return (
    <div className="flex min-h-0 flex-1 flex-col">
      <div className="scroll flex-1">
        <div className="mx-auto my-0 w-[min(1440px,100%)] p-[clamp(20px,3vw,44px)]">
          <div className="mb-5 flex flex-wrap items-center gap-3.5">
            <IconButton onClick={() => goToMethod(method)} title="Retourner à la méthode">
              <ChevronLeft size={24} className="text-accent" />
            </IconButton>
            <div className="type-ic size-9 text-fg2">
              <Video size={18} />
            </div>
            <h1 className="display m-0 min-w-56 flex-1 text-[clamp(20px,3vw,44px)]">
              {lesson.title}
            </h1>
            <Button variant={!done ? 'primary' : undefined} onClick={handleToggleDone}>
              <Check size={17} /> {done ? 'Terminé' : 'Marquer terminé'}
            </Button>
          </div>

          {lesson.tabs.length > 0 ? (
            <div className="vp-grid">
              <div className="min-w-0">{videoPlayer}</div>
              <aside className="vp-aside">
                <div className="eyebrow mb-3 text-xs">
                  {lesson.tabs.length > 1
                    ? `Exercices · ${lesson.tabs.length}`
                    : 'Exercice de la leçon'}
                </div>
                <div className="flex flex-col gap-2.5">
                  {lesson.tabs.map((tab) => (
                    <button
                      key={tab.id}
                      type="button"
                      className="card ex-card"
                      onClick={() => openTab(lesson, chapter, tab)}
                    >
                      <div className="flex items-center gap-3">
                        <div
                          className="type-ic tab size-8 flex-initial"
                          style={{ width: 34, height: 34, flex: '0 0 auto' }}
                        >
                          <Music size={18} />
                        </div>
                        <div className="display min-w-0 flex-1 text-base/tight">{tab.title}</div>
                        <ChevronRight size={18} className="ex-arrow" />
                      </div>
                    </button>
                  ))}
                </div>
              </aside>
            </div>
          ) : (
            videoPlayer
          )}
        </div>
      </div>
      {chapter.lessonsStatus.length > 1 && (
        <div className="flex-initial border-border border-t bg-bg2 px-[clamp(16px,3vw,28px)] py-3">
          <div className="mx-auto my-0 flex w-[min(1200px,100%)] items-center justify-between gap-4">
            <div className="min-w-0 flex-[0_0_clamp(168px,32%,300px)]">
              {previousLesson && (
                <NavLesson
                  dir="prev"
                  title={previousLesson.title}
                  onClick={() => openLesson(previousLesson, chapter)}
                />
              )}
            </div>
            <div className="mono whitespace-nowrap text-center text-fg3 text-xs">
              Partie {chapter.lessonsStatus.findIndex((l) => l.lessonId === lesson.id) + 1} /{' '}
              {chapter.lessonsStatus.length}
            </div>
            <div className="min-w-0 flex-[0_0_clamp(168px,32%,300px)]">
              {nextLesson && (
                <NavLesson
                  dir="next"
                  title={nextLesson.title}
                  onClick={() => openLesson(nextLesson, chapter)}
                />
              )}
            </div>
          </div>
        </div>
      )}
    </div>
  );
}
