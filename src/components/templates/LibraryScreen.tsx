import { useEffect, useState } from "react";
import type { Method } from "@/types/model";
import { useBreadcrumb } from "../../contexts/BreadcrumbContext";
import { listMethods } from "../../lib/ipc";
import SearchField from "../molecules/SearchField";
import MethodCard from "../organisms/MethodCard";

type LibraryScreenProps = {
  onOpen: () => void;
};

export default function LibraryScreen({ onOpen }: LibraryScreenProps) {
  const [q, setQ] = useState("");
  const [methods, setMethods] = useState<Method[]>([]);
  const [lib, setLib] = useState<Method[]>([]);
  const { dispatch: dispatchBreadcrumb } = useBreadcrumb();

  dispatchBreadcrumb({
    type: "replace",
    payload: [{ label: "Bibliothèque" }],
  });

  useEffect(() => {
    listMethods().then(setMethods);
  }, []);

  useEffect(() => {
    setLib(
      methods.filter((m) =>
        m.title
          .toLowerCase()
          .normalize("NFD")
          .replace(/\p{Diacritic}/gu, "")
          .includes(
            q
              .toLowerCase()
              .normalize("NFD")
              .replace(/\p{Diacritic}/gu, ""),
          ),
      ),
    );
  }, [methods, q]);

  return (
    <div className="flex-1 overflow-y-auto overflow-x-hidden">
      <div className="m-0 mx-auto max-w-7xl p-[clamp(24px,4vw,48px)]">
        <div className="mb-7 flex flex-wrap items-end justify-between gap-5">
          <div>
            <div className="mn-2 font-bold text-fg3 text-xs uppercase tracking-widest">
              Bibliothèque
            </div>
            <h1 className="display m-0 text-[clamp(34px,4vw,52px)]">Tes méthodes</h1>
          </div>
          <SearchField
            value={q}
            onChange={(e) => setQ(e.target.value)}
            placeholder="Rechercher une méthode…"
          />
        </div>

        <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-(--gap)">
          {lib.map((m) => (
            <MethodCard key={m.id} method={m} onOpen={onOpen} />
          ))}
        </div>
      </div>
    </div>
  );
}
