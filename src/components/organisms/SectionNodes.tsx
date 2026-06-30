import LessonRow from "@/components/molecules/LessonRow";
import cn from "@/lib/cn";
import { type Chapter, countLessons } from "@/lib/method-view";
import type { SectionItem } from "@/types/model";

type SectionNodesProps = {
  items: SectionItem[];
  chapter: Chapter;
  currentPartId: string | undefined;
  depth?: number;
};

export default function SectionNodes({
  items,
  chapter,
  currentPartId,
  depth = 0,
}: SectionNodesProps) {
  return items.map((item) => {
    if (item.type === "lesson") {
      return (
        <LessonRow
          key={item.id}
          lesson={item}
          status={chapter.lessonsStatus.find((stat) => stat.lessonId === item.id)?.status || "todo"}
          chapter={chapter}
          active={item.id === currentPartId}
        />
      );
    }

    const isEpisode = depth === 0;

    return (
      <div key={item.id} className={cn("mb-2", depth === 0 && "mb-3.5")}>
        <div
          className={cn("flex items-center gap-2 px-0.5 pt-1 pb-1.5", depth === 0 && "pt-1.5 pb-2")}
        >
          <span
            className={cn("size-1.5 flex-initial rounded-full bg-fg3", isEpisode && "bg-accent")}
          />
          <div className={cn("section-title", isEpisode && "section-title--episode")}>
            {item.title}
          </div>
          <span className="mono text-fg3 text-xs">{countLessons([item])}</span>
        </div>
        <div className="ml-2 border-l border-l-border pl-3">
          <SectionNodes
            items={item.items}
            chapter={chapter}
            currentPartId={currentPartId}
            depth={depth + 1}
          />
        </div>
      </div>
    );
  });
}
