import { Check, ChevronLeft, Download, Music } from 'lucide-react';
import { useState } from 'react';
import Button from '@/components/atoms/Button';
import cn from '@/lib/cn';
import { computeMethodColors } from '@/lib/colors';
import { saveMethod } from '@/lib/ipc';
import { countLessons, countTabs } from '@/lib/method-view';
import type { Method } from '@/types/model';

type ScanReviewProps = {
  foundMethods: Method[];
  onImport: () => void;
  onBack: () => void;
};

export default function ScanReview({ foundMethods, onImport, onBack }: ScanReviewProps) {
  const [excluded, setExcluded] = useState(() => new Set<string>());
  const toggle = (id: string) =>
    setExcluded((prev) => {
      const n = new Set(prev);
      n.has(id) ? n.delete(id) : n.add(id);
      return n;
    });

  const kept = foundMethods.filter((m) => !excluded.has(m.id));
  const totVideos = kept.reduce((s, m) => s + countLessons(m.items), 0);

  const handleImport = async () => {
    for (const method of kept) {
      await saveMethod(method);
    }

    onImport();
  };

  return (
    <div
      className="card p-7"
      style={{ width: 'min(460px, 100%)', animation: 'fadeUp .35s var(--ease)' }}
    >
      <div className="eyebrow mb-1.5">Étape 3 / 3 · Méthodes trouvées</div>
      <h2 className="display m-0 mb-1 text-2xl">
        {foundMethods.length} méthode{foundMethods.length > 1 ? 's' : ''} détectée
        {foundMethods.length > 1 ? 's' : ''}
      </h2>
      <p className="m-0 mb-3 text-fg2 text-sm/normal">
        Décoche celles que tu ne veux pas importer.
      </p>

      <div className="relative mb-3 max-h-64 overflow-y-auto rounded-lg border border-border bg-bg3 p-1.5">
        {foundMethods.map((m) => {
          const on = !excluded.has(m.id);
          const [color] = computeMethodColors(m.title);

          return (
            <button
              key={m.id}
              type="button"
              onClick={() => toggle(m.id)}
              className={cn('row-btn min-h-14 opacity-100', !on && 'opacity-50')}
            >
              <span
                aria-hidden
                className={cn(
                  'flex size-5 flex-initial items-center justify-center rounded border-2 border-accent bg-accent text-accentink transition-all',
                  !on && 'border-border2 bg-transparent',
                )}
              >
                {on && <Check size={13} />}
              </span>
              <span
                className="flex size-9 flex-initial items-center justify-center rounded-sm text-white"
                style={{ backgroundColor: color }}
              >
                <Music size={17} />
              </span>
              <div className="min-w-0 flex-1">
                <div className="mb-0.5 min-w-0 flex-1 overflow-hidden text-ellipsis text-nowrap font-medium text-sm">
                  {m.title}
                </div>
                <div className="tabular-enums text-fg3 text-xs">
                  {countLessons(m.items)} vidéos · {countTabs(m.items)} tabs
                </div>
              </div>
            </button>
          );
        })}
      </div>

      <div className="flex items-center gap-3">
        <button
          type="button"
          onClick={onBack}
          className="flex cursor-pointer items-center gap-1 border-0 bg-none p-0 text-fg3 text-xs"
        >
          <ChevronLeft size={15} /> Autre dossier
        </button>
        <div className="flex-1" />
        <div className="text-right text-fg3 text-xs/snug">
          {kept.length}/{foundMethods.length} · {totVideos} vidéos
        </div>
        <Button variant="primary" disabled={kept.length === 0} onClick={handleImport}>
          <Download size={16} /> Importer
        </Button>
      </div>
    </div>
  );
}
