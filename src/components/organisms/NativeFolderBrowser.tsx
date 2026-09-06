import type { IconType } from '@icons-pack/react-simple-icons';
import type { LucideIcon } from 'lucide-react';
import { AlertCircle, ChevronRight, Folder, Search } from 'lucide-react';
import React from 'react';
import Button from '@/components/atoms/Button';
import Chip from '@/components/atoms/Chip';
import Spinner from '@/components/atoms/Spinner';
import cn from '@/lib/cn';
import type { FolderEntry } from '@/lib/ipc';

export type Crumb = { id: string; name: string };

type NativeFolderBrowserProps = {
  title: string;
  providerLabel: string;
  ProviderIcon: LucideIcon | IconType;
  crumbs: Crumb[];
  entries: FolderEntry[];
  loading: boolean;
  error: string | null;
  onGoTo: (index: number) => void;
  onEnter: (entry: FolderEntry) => void;
  onScan: () => void;
};

/**
 * Browse-state UI for providers with full-account access (pCloud, Dropbox): breadcrumb
 * navigation over an in-app folder listing. gdrive skips this entirely — its root folder
 * is already granted via Google's Picker before FolderPicker even mounts (cf. ConnectScreen).
 */
export default function NativeFolderBrowser({
  title,
  providerLabel,
  ProviderIcon,
  crumbs,
  entries,
  loading,
  error,
  onGoTo,
  onEnter,
  onScan,
}: NativeFolderBrowserProps) {
  const breadcrumbSegs = crumbs.slice(1); // skip root — shown as cloud provider chip

  return (
    <div className="card w-[min(440px,100%)] animate-[fadeUp_.35s_var(--ease)] p-7">
      <div className="eyebrow mb-1.5">Étape 2 / 3 · {providerLabel} connecté</div>
      <h2 className="display m-0 mb-1 text-2xl">{title}</h2>
      <p className="m-0 mb-4 text-fg2 text-xs leading-normal">
        Choisis un dossier : on en analysera le contenu pour y repérer les méthodes.
      </p>

      {/* fil d'ariane */}
      <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
        <Chip as="button" className="h-7 cursor-pointer" onClick={() => onGoTo(0)}>
          <ProviderIcon size={13} /> {providerLabel}
        </Chip>
        {breadcrumbSegs.map((seg, i) => {
          const idx = i + 1; // real index in crumbs
          const isLast = idx === crumbs.length - 1;
          return (
            <React.Fragment key={idx}>
              <ChevronRight size={13} className="text-fg3" />
              <button
                type="button"
                onClick={() => onGoTo(idx)}
                className={cn(
                  'cursor-pointer border-0 bg-none bg-transparent p-0 font-medium text-fg2 text-xs',
                  isLast && 'font-semibold text-fg',
                )}
              >
                {seg.name}
              </button>
            </React.Fragment>
          );
        })}
      </div>

      {/* listing */}
      <div className="relative mb-4 h-65 overflow-y-auto rounded-lg border border-border bg-bg3 p-1.5">
        {loading && (
          <div className="flex h-full items-center justify-center gap-2 text-fg3 text-xs">
            <Spinner /> Chargement…
          </div>
        )}
        {!loading && error && (
          <div className="flex items-center gap-2 p-5 text-red-400 text-xs">
            <AlertCircle size={14} /> {error}
          </div>
        )}
        {!loading && !error && entries.length === 0 && (
          <div className="p-5 text-center text-fg3 text-xs">Dossier vide</div>
        )}
        {!loading &&
          !error &&
          entries.map((entry) => (
            <button
              type="button"
              key={entry.folderid}
              onClick={() => onEnter(entry)}
              className="row-btn min-h-10"
            >
              <Folder size={18} className="text-fg3" />
              <div className="min-w-0 flex-1 overflow-hidden text-ellipsis whitespace-nowrap font-semibold text-sm">
                {entry.name}
              </div>
              <ChevronRight size={15} className="text-fg3" />
            </button>
          ))}
      </div>

      <div className="flex items-center gap-3">
        <div className="flex-1 text-fg3 text-sm">
          {!loading && !error && `${entries.length} sous-dossier${entries.length !== 1 ? 's' : ''}`}
        </div>
        <Button variant="primary" disabled={loading || !!error} onClick={onScan}>
          <Search size={17} /> Scanner ce dossier
        </Button>
      </div>
    </div>
  );
}
