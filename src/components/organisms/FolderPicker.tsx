import { AlertCircle, ChevronRight, Cloud, Folder, Search } from "lucide-react";
import React, { useEffect, useState } from "react";
import cn from "../../lib/cn";
import type { FolderEntry } from "../../lib/ipc";
import { listFolder } from "../../lib/ipc";
import type { Method } from "../../types/model";
import Button from "../atoms/Button";
import Chip from "../atoms/Chip";
import Spinner from "../atoms/Spinner";
import ScanProgress from "./ScanProgress";
import ScanReview from "./ScanReview";

type Crumb = { id: number; name: string };

type FolderPickerProps = {
  onConnected: () => void;
};

type State = "browse" | "scan" | "review";

export default function FolderPicker({ onConnected }: FolderPickerProps) {
  const [state, setState] = useState<State>("browse");
  const [path, setPath] = useState<string>("/");
  // crumbs[0] is always root. The current folder is the last crumb.
  const [crumbs, setCrumbs] = useState<Crumb[]>([{ id: 0, name: "pCloud" }]);
  const [entries, setEntries] = useState<FolderEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revealedMethods, setRevealedMethods] = useState<Method[]>([]);

  const currentId = crumbs[crumbs.length - 1]?.id ?? 0;

  useEffect(() => {
    let cancelled = false;

    setLoading(true);
    setError(null);

    listFolder(currentId)
      .then((folders) => {
        if (!cancelled) {
          setEntries(folders);
          setPath(
            "/" +
              crumbs
                .map((c) => c.name)
                .slice(1)
                .join("/"),
          );
          setLoading(false);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(String(err));
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [currentId, crumbs.map]);

  const enter = (entry: FolderEntry) =>
    setCrumbs((prev) => [...prev, { id: entry.folderid, name: entry.name }]);

  const goTo = (index: number) => setCrumbs((prev) => prev.slice(0, index + 1));

  const breadcrumbSegs = crumbs.slice(1); // skip root — shown as "pCloud" chip

  const handleScanDone = (methods: Method[]) => {
    setRevealedMethods(methods);
    setState("review");
  };

  if (state === "scan") {
    return (
      <ScanProgress
        folderId={currentId}
        path={path}
        onDone={handleScanDone}
        onCancel={() => setState("browse")}
      />
    );
  }

  if (state === "review") {
    return (
      <ScanReview
        foundMethods={revealedMethods}
        onImport={onConnected}
        onBack={() => setState("browse")}
      />
    );
  }

  return (
    <div className="relative w-[min(440px,100%)] animate-[fadeUp_.35s_var(--ease)] rounded-lg border border-border bg-surface p-7">
      <div className="mb-1.5 font-bold text-fg3 text-xs uppercase tracking-wide">
        Étape 2 / 3 · pCloud connecté
      </div>
      <h2 className="display m-0 mb-1 text-2xl">Où sont tes méthodes ?</h2>
      <p className="m-0 mb-4 text-fg2 text-xs leading-normal">
        Choisis un dossier : on en analysera le contenu pour y repérer les méthodes.
      </p>

      {/* fil d'ariane */}
      <div className="mb-2 flex flex-wrap items-center gap-2 text-xs">
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
                type="button"
                onClick={() => goTo(idx)}
                className={cn(
                  "cursor-pointer border-0 bg-none bg-transparent p-0 font-medium text-fg2 text-xs",
                  isLast && "font-semibold text-fg",
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
              onClick={() => enter(entry)}
              className="flex min-h-10 w-full items-center gap-3 rounded-sm border border-transparent bg-transparent px-3 py-1.5 text-left text-fg transition-colors hover:bg-chip"
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
          {!loading && !error && `${entries.length} sous-dossier${entries.length !== 1 ? "s" : ""}`}
        </div>
        <Button variant="primary" disabled={loading || !!error} onClick={() => setState("scan")}>
          <Search size={17} /> Scanner ce dossier
        </Button>
      </div>
    </div>
  );
}
