import { ChevronRight, Cloud, Folder, AlertCircle } from "lucide-react";
import Button from "../atoms/Button";
import Spinner from "../atoms/Spinner";
import React, { useEffect, useState } from "react";
import Chip from "../atoms/Chip";
import cn from "../../lib/cn";
import { listFolder } from "../../lib/ipc";
import type { FolderEntry } from "../../lib/ipc";

type Crumb = { id: number; name: string };

type FolderPickerProps = {
    onConnected: () => void;
};

export default function FolderPicker({ onConnected }: FolderPickerProps) {
    // crumbs[0] is always root. The current folder is the last crumb.
    const [crumbs, setCrumbs] = useState<Crumb[]>([{ id: 0, name: 'pCloud' }]);
    const [entries, setEntries] = useState<FolderEntry[]>([]);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState<string | null>(null);

    const currentId = crumbs[crumbs.length - 1]!.id;

    useEffect(() => {
        let cancelled = false;

        setLoading(true);
        setError(null);

        listFolder(currentId)
            .then(folders => {
                if (!cancelled) {
                    setEntries(folders);
                    setLoading(false);
                }
            })
            .catch(err => {
                if (!cancelled) {
                    setError(String(err));
                    setLoading(false);
                }
            });

        return () => { cancelled = true; };
    }, [currentId]);

    const enter = (entry: FolderEntry) =>
        setCrumbs(prev => [...prev, { id: entry.folderid, name: entry.name }]);

    const goTo = (index: number) =>
        setCrumbs(prev => prev.slice(0, index + 1));

    const breadcrumbSegs = crumbs.slice(1); // skip root — shown as "pCloud" chip

    return (
        <div className="bg-surface rounded-lg border border-border relative p-7 w-[min(440px,100%)] animate-[fadeUp_.35s_var(--ease)]">
            <div className="text-xs font-bold tracking-wide uppercase text-fg3 mb-1.5">Étape 2 / 2 · pCloud connecté</div>
            <h2 className="display text-2xl m-0 mb-1">Où sont tes méthodes ?</h2>
            <p className="m-0 mb-4 text-fg2 text-xs leading-normal">
                Parcours ton cloud et choisis le dossier qui contient tes méthodes.
            </p>

            {/* fil d'ariane */}
            <div className="flex items-center gap-2 mb-2 text-xs flex-wrap">
                <Chip as="button" className="h-7 cursor-pointer" onClick={() => goTo(0)}>
                    <Cloud size={13} /> pCloud
                </Chip>
                {breadcrumbSegs.map((seg, i) => {
                    const idx = i + 1; // real index in crumbs
                    const isLast = idx === crumbs.length - 1;
                    return (
                        <React.Fragment key={idx}>
                            <ChevronRight size={13} className="text-fg3" />
                            <button
                                onClick={() => goTo(idx)}
                                className={cn(
                                    'bg-none bg-transparent border-0 cursor-pointer p-0 text-xs text-fg2 font-medium',
                                    isLast && 'text-fg font-semibold',
                                )}
                            >
                                {seg.name}
                            </button>
                        </React.Fragment>
                    );
                })}
            </div>

            {/* listing */}
            <div className="rounded-lg border border-border relative bg-bg3 h-55 p-1.5 mb-4 overflow-y-auto">
                {loading && (
                    <div className="flex items-center justify-center h-full text-fg3 gap-2 text-xs">
                        <Spinner /> Chargement…
                    </div>
                )}
                {!loading && error && (
                    <div className="flex items-center gap-2 p-5 text-xs text-red-400">
                        <AlertCircle size={14} /> {error}
                    </div>
                )}
                {!loading && !error && entries.length === 0 && (
                    <div className="text-xs text-fg3 p-5 text-center">Dossier vide</div>
                )}
                {!loading && !error && entries.map((entry, i) => (
                    <button key={i} onClick={() => enter(entry)}
                        className="flex items-center gap-3 w-full min-h-10 py-1.5 px-3 rounded-sm bg-transparent border border-transparent text-fg text-left hover:bg-chip transition-colors">
                        <Folder size={18} className="text-fg3" />
                        <div className="flex-1 min-w-0 overflow-hidden text-ellipsis whitespace-nowrap text-sm font-semibold">{entry.name}</div>
                        <ChevronRight size={15} className="text-fg3" />
                    </button>
                ))}
            </div>

            <div className="flex items-center gap-3">
                <div className="flex-1 text-sm text-fg3">
                    {!loading && !error && `${entries.length} sous-dossier${entries.length !== 1 ? 's' : ''}`}
                </div>
                <Button variant="primary" disabled={loading || !!error} onClick={onConnected}>
                    Choisir ce dossier <ChevronRight size={17} />
                </Button>
            </div>
        </div>
    );
}
