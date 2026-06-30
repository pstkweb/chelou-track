import { Guitar } from "lucide-react";
import Button from "@/components/atoms/Button";
import { computeMethodColors } from "@/lib/helpers";
import { progressPct } from "@/lib/ipc";
import type { Method, SectionItem } from "@/types/model";

const countVideos = (...items: SectionItem[]) => {
  let total = 0;

  for (const item of items) {
    if (item.type === "lesson") {
      total += item.videos.length;
    }

    if (item.type === "section") {
      total += countVideos(...item.items);
    }
  }

  return total;
};

const countTabs = (...items: SectionItem[]) => {
  let total = 0;

  for (const item of items) {
    if (item.type === "lesson") {
      total += item.tabs.length;
    }

    if (item.type === "section") {
      total += countTabs(...item.items);
    }
  }

  return total;
};

const countBackingTracks = (...items: SectionItem[]) => {
  let total = 0;

  for (const item of items) {
    if (item.type === "lesson") {
      total += item.backingGroups.length;
    }

    if (item.type === "section") {
      total += countBackingTracks(...item.items);
    }
  }

  return total;
};

type MethodCardProps = {
  method: Method;
  onOpen: (m: Method) => void;
};

export default function MethodCard({ method, onOpen }: MethodCardProps) {
  const started = Object.keys(method.progress).length > 0;
  const fmtPct = () => `${(progressPct(method) * 100).toFixed()}%`;
  const [c1, c2] = computeMethodColors(method.title);

  return (
    <div className="card card-lift flex cursor-pointer flex-col overflow-hidden p-0">
      <button
        type="button"
        className="absolute inset-0 z-0"
        onClick={() => onOpen(method)}
        aria-label={`Ouvrir ${method.title}`}
      />
      <div className="relative z-10 flex flex-col">
        <div
          className="relative h-28 overflow-hidden text-white"
          style={{ background: `linear-gradient(150deg, ${c1}, ${c2})` }}
        >
          <div className="absolute inset-0 bg-[repeating-linear-gradient(180deg,rgba(255,255,255,.13)_0_1px,transparent_1px_17px)]" />
          <div className="absolute top-4 left-4 opacity-90">
            <Guitar size={30} />
          </div>
          <h3 className="display absolute right-4 bottom-3 left-4 m-0 text-xl [text-shadow:0_1px_10px_rgba(0,0,0,.35)]">
            {method.title}
          </h3>
        </div>
        <div className="flex flex-col gap-3 p-(--pad)">
          <div className="flex flex-wrap gap-4 text-fg3 text-xs">
            <span>
              <b className="text-fg2">{countVideos(...method.items)}</b> vidéos
            </span>
            <span>
              <b className="text-fg2">{countTabs(...method.items)}</b> tabs
            </span>
            <span>
              <b className="text-fg2">{countBackingTracks(...method.items)}</b> backings
            </span>
          </div>
          {started ? (
            <div className="flex items-center gap-3">
              <div className="flex-1">
                <div className="pbar">
                  <i style={{ width: fmtPct() }} />
                </div>
              </div>
              <span className="mono text-fg2 text-xs">{fmtPct()}</span>
              <Button
                variant="primary"
                className="h-9 p-0 px-3.5 text-sm"
                onClick={() => onOpen(method)}
              >
                Reprendre
              </Button>
            </div>
          ) : (
            <div className="flex items-center gap-3">
              <span className="flex-1 text-fg3 text-xs">Pas encore ouverte</span>
              <Button className="h-9" onClick={() => onOpen(method)}>
                Ouvrir la méthode
              </Button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
