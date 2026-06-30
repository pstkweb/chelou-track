import { Guitar, Play } from "lucide-react";
import Button from "@/components/atoms/Button";
import ProgressRing from "@/components/atoms/ProgressRing";
import { useNavigation } from "@/contexts/NavigationContext";
import { computeMethodColors } from "@/lib/helpers";
import type { Chapter, MethodStats } from "@/lib/method-view";
import type { Lesson } from "@/types/model";

type MethodHeroProps = {
  currentLesson: { lesson: Lesson; chapter: Chapter } | undefined;
  methodStats: MethodStats;
  title: string;
};

export default function MethodHero({ title, methodStats, currentLesson }: MethodHeroProps) {
  const { openPart } = useNavigation();
  const [c1, c2] = computeMethodColors(title);
  const progressPct =
    methodStats.videosCount === 0 ? 0 : methodStats.videosDone / methodStats.videosCount;

  return (
    <div className="flex flex-wrap items-center gap-[clamp(18px,3vw,36px)]">
      <div
        className="relative size-40 flex-initial overflow-hidden rounded text-white shadow"
        style={{
          background: `linear-gradient(150deg, ${c1}, ${c2})`,
        }}
      >
        <div
          className="absolute inset-0"
          style={{
            background:
              "repeating-linear-gradient(180deg, rgba(255,255,255,.13) 0 1px, transparent 1px 22px)",
          }}
        />
        <div className="absolute inset-0 flex items-center justify-center opacity-90">
          <Guitar size={64} />
        </div>
      </div>
      <div className="min-w-72 flex-1">
        <h1 className="display m-0 mb-2 text-[clamp(36px,4.5vw,60px)]">{title}</h1>
        <p className="m-0 mb-4 max-w-xl text-fg2 text-lg/normal">
          {methodStats.chaptersCount} chapitres · {methodStats.videosCount} vidéos ·{" "}
          {methodStats.tabsCount} tablatures.
        </p>
        <div className="flex flex-wrap items-center gap-4">
          {currentLesson && (
            <Button
              variant="primary"
              size="lg"
              onClick={() => openPart(currentLesson.lesson, currentLesson.chapter, "video")}
            >
              <Play size={18} /> Reprendre · {currentLesson.lesson.title.slice(0, 28)}
              {currentLesson.lesson.title.length > 28 ? "…" : ""}
            </Button>
          )}
          <div className="flex items-center gap-3">
            <ProgressRing value={progressPct} size={48}>
              {Math.round(progressPct * 100)}
            </ProgressRing>
            <div className="text-fg2 text-sm/snug">
              <b className="text-fg">
                {methodStats.videosDone}/{methodStats.videosCount}
              </b>{" "}
              vidéos
              <br />
              terminées
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}
