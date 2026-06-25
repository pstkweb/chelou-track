import { Guitar } from "lucide-react";
import type { Method, SectionItem } from "@/types/model";
import { progressPct } from "../../lib/ipc";
import Button from "../atoms/Button";

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

// Teinte stable dérivée du titre. Hash à avalanche (xorshift) + angle d'or :
// de petites différences de titre écartent fortement la teinte → pas de quasi-collisions.
const computeColors = (title: string): [string, string] => {
  let h = 0x811c9dc5;

  for (let i = 0; i < title.length; i++) {
    h ^= title.charCodeAt(i);
    h = Math.imul(h, 0x01000193);
    h ^= h >>> 7;
  }
  h = h >>> 0;

  const hue = (h * 137.508) % 360; // angle d'or → répartition maximale
  const chroma = 0.12 + (((h >>> 8) % 100) / 100) * 0.05; // 0.12–0.17
  const light = 0.6 + (((h >>> 16) % 100) / 100) * 0.08; // 0.60–0.68

  return [
    `oklch(${light.toFixed(3)} ${chroma.toFixed(3)} ${hue})`,
    `oklch(${(light - 0.22).toFixed(3)} ${(chroma - 0.03).toFixed(3)} ${(hue + 18) % 360})`,
  ];
};

type MethodCardProps = {
  method: Method;
  onOpen: (m: Method) => void;
};

export default function MethodCard({ method, onOpen }: MethodCardProps) {
  const started = Object.keys(method.progress).length > 0;
  const fmtPct = () => `${(progressPct(method) * 100).toFixed()}%`;
  const [c1, c2] = computeColors(method.title);

  return (
    // biome-ignore lint/a11y/noStaticElementInteractions: not useful
    // biome-ignore lint/a11y/useKeyWithClickEvents: not useful
    <div
      className="relative flex cursor-pointer flex-col overflow-hidden rounded-lg border border-border bg-surface p-0 transition-[transform,border-color] duration-180 ease-(--ease) hover:-translate-y-0.75 hover:border-border2"
      onClick={() => onOpen(method)}
    >
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
              <div className="h-1.5 overflow-hidden rounded-full bg-chip">
                <i
                  className="block h-full rounded-full bg-[linear-gradient(90deg,var(--accent),var(--accent-2))]"
                  style={{ width: fmtPct() }}
                />
              </div>
            </div>
            <span className="mono text-fg2 text-xs">{fmtPct()}</span>
            <Button
              variant="primary"
              className="h-9 p-0 px-3.5 text-sm"
              onClick={(e) => {
                e.stopPropagation();
                onOpen(method);
              }}
            >
              Reprendre
            </Button>
          </div>
        ) : (
          <div className="flex items-center gap-3">
            <span className="flex-1 text-fg3 text-xs">Pas encore ouverte</span>
            <Button
              className="h-9"
              onClick={(e) => {
                e.stopPropagation();
                onOpen(method);
              }}
            >
              Ouvrir la méthode
            </Button>
          </div>
        )}
      </div>
    </div>
  );
}
