import { Guitar, Trash2 } from 'lucide-react';
import { useState } from 'react';
import Button from '@/components/atoms/Button';
import cn from '@/lib/cn';
import { computeMethodColors } from '@/lib/colors';
import { countBackingTracks, countLessons, countTabs, methodProgressPct } from '@/lib/method-view';
import { PROVIDERS } from '@/lib/providers';
import type { Method } from '@/types/model';

type MethodCardProps = {
  method: Method;
  isAvailable: boolean;
  onDelete: (method: Method) => void;
  onOpen: (method: Method) => void;
};

export default function MethodCard({ method, isAvailable, onDelete, onOpen }: MethodCardProps) {
  const [isConfirming, setIsConfirming] = useState(false);
  const started = Object.keys(method.progress).length > 0;
  const fmtPct = () => `${(methodProgressPct(method) * 100).toFixed()}%`;
  const [c1, c2] = computeMethodColors(method.title);

  const handleOpen = () => {
    if (isAvailable) onOpen(method);
  };
  const handleEnterPress = (e: React.KeyboardEvent) => {
    if (e.key === 'Enter') {
      handleOpen();
    }
  };

  return (
    <div
      className={cn(
        'card card-lift flex flex-col overflow-hidden p-0',
        isAvailable ? 'cursor-pointer' : 'grayscale',
      )}
      onClick={handleOpen}
      onKeyDown={handleEnterPress}
    >
      <div className="relative z-10 flex flex-col">
        <div
          className="relative h-28 overflow-hidden text-white"
          style={{ background: `linear-gradient(150deg, ${c1}, ${c2})` }}
        >
          <div className="absolute inset-0 bg-[repeating-linear-gradient(180deg,rgba(255,255,255,.13)_0_1px,transparent_1px_17px)]" />
          <div className="absolute top-4 left-4 opacity-90">
            <Guitar size={30} />
          </div>
          <button
            type="button"
            className="absolute top-2.5 right-2.5 flex size-7.5 cursor-pointer items-center justify-center rounded-full border-0 bg-white/30 text-white backdrop-blur-xs"
            title="Supprimer cette méthode"
            onClick={(e) => {
              e.stopPropagation();

              setIsConfirming(true);
            }}
          >
            <Trash2 size={15} />
          </button>
          <h3 className="display absolute right-4 bottom-3 left-4 m-0 text-xl [text-shadow:0_1px_10px_rgba(0,0,0,.35)]">
            {method.title}
          </h3>

          {isConfirming && (
            <div
              className="absolute inset-0 z-2 flex flex-col items-center justify-center gap-2.5 bg-[#14100e]/90 p-3.5 text-center"
              onClick={(e) => e.stopPropagation()}
              onKeyUp={() => {}}
            >
              <div className="font-semibold text-sm">Supprimer « {method.title} » ?</div>
              <div className="flex gap-2">
                <Button
                  variant="ghost"
                  className="h-7.5 px-3 py-0 text-white text-xs"
                  onClick={() => setIsConfirming(false)}
                >
                  Annuler
                </Button>
                <Button
                  className="h-7.5 bg-[#e5484d] px-3 py-0 text-white text-xs"
                  onClick={() => onDelete(method)}
                >
                  Supprimer
                </Button>
              </div>
            </div>
          )}
        </div>
        <div className="flex flex-col gap-3 p-(--pad)">
          <div className="flex flex-wrap gap-4 text-fg3 text-xs">
            <span>
              <b className="text-fg2">{countLessons(method.items)}</b> vidéos
            </span>
            <span>
              <b className="text-fg2">{countTabs(method.items)}</b> tabs
            </span>
            <span>
              <b className="text-fg2">{countBackingTracks(method.items)}</b> backings
            </span>
          </div>
          {!isAvailable ? (
            <div className="flex items-center gap-3">
              <span className="flex-1 text-fg3 text-xs">
                Reconnecte {PROVIDERS[method.source.provider].label} pour y accéder
              </span>
            </div>
          ) : started ? (
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
