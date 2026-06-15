import { listen } from "@tauri-apps/api/event";
import { AlertCircle } from "lucide-react";
import { useEffect, useState } from "react";
import type { Method } from "@/types/model";
import { type ScanProgressEvent, scanMethod } from "../../lib/ipc";
import Button from "../atoms/Button";
import Spinner from "../atoms/Spinner";

type ScanProgressProps = {
  folderId: number;
  path: string;
  onDone: (methods: Method[]) => void;
  onCancel: () => void;
};

export default function ScanProgress({ folderId, path, onDone, onCancel }: ScanProgressProps) {
  const [scannedFolders, setScannedFolders] = useState(0);
  const [foundMethods, setFoundMethods] = useState(0);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let unlisten: (() => void) | undefined;

    listen<ScanProgressEvent>("scan:progress", (e) => {
      setScannedFolders(e.payload.foldersVisited);
      setFoundMethods(e.payload.methodsFound);
    }).then((fn) => {
      unlisten = fn;
    });

    scanMethod(folderId)
      .then((methods) => {
        unlisten?.();

        onDone(methods);
      })
      .catch((err) => {
        unlisten?.();

        setError(String(err));
      });

    return () => {
      unlisten?.();
    };
  }, [onDone, folderId]);

  return (
    <div
      className="relative rounded-lg border border-border bg-surface p-7"
      style={{ width: "min(440px, 100%)", animation: "fadeUp .35s var(--ease)" }}
    >
      <div className="mb-1.5 font-bold text-fg3 text-xs uppercase tracking-widest">
        Étape 2 / 3 · Analyse
      </div>
      <h2 className="display m-0 mb-1 text-2xl">Analyse du dossier…</h2>
      <p className="m-0 mb-5 text-fg2 text-sm/normal">
        On parcourt <span className="mono text-fg3">{path}</span> à la recherche de structures de
        méthode (chapitres, vidéos, tablatures).
      </p>

      <div className="mb-5 flex items-center gap-3.5">
        <Spinner className="size-14 border-4" />
        <div className="min-w-0 flex-1">
          <div className="font-semibold text-sm">
            {scannedFolders.toLocaleString("fr-FR")} dossiers inspectés
          </div>
          <div className="text-fg3 text-xs">
            {foundMethods > 0 ? (
              <>
                <b className="text-accent">{foundMethods}</b> méthode(s) détectée(s)
              </>
            ) : (
              "Recherche en cours…"
            )}
          </div>
        </div>
      </div>

      {error && (
        <div className="flex items-center gap-2 p-5 text-red-400 text-xs">
          <AlertCircle size={14} /> {error}
        </div>
      )}

      <div className="flex justify-center">
        <Button variant="ghost" onClick={onCancel}>
          Annuler
        </Button>
      </div>
    </div>
  );
}
