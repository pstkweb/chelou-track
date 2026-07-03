import { SkipBack, SkipForward } from "lucide-react";
import cn from "@/lib/cn";

type NavLessonProps = {
  dir: "next" | "prev";
  title: string;
  onClick: () => void;
};

export default function NavLesson({ dir, title, onClick }: NavLessonProps) {
  return (
    <button
      className={cn(
        "card flex w-full cursor-pointer flex-row items-center gap-3 border-border bg-bg3 px-3.5 py-3 text-left text-fg transition-colors duration-150 ease-(--ease) hover:border-border2",
        dir === "next" && "flex-row-reverse text-right",
      )}
      type="button"
      onClick={onClick}
    >
      {dir === "prev" ? (
        <SkipBack size={20} className="text-accent" />
      ) : (
        <SkipForward size={20} className="text-accent" />
      )}
      <div className="min-w-0 flex-1">
        <div className="font-semibold text-fg2 text-xs uppercase tracking-wider">
          {dir === "prev" ? "Précédent" : "Suivant"}
        </div>
        <div className="overflow-hidden text-ellipsis whitespace-nowrap font-semibold text-fg text-sm">
          {title}
        </div>
      </div>
    </button>
  );
}
