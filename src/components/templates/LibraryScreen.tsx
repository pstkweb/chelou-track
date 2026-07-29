import { useEffect, useState } from 'react';
import SearchField from '@/components/molecules/SearchField';
import MethodCard from '@/components/organisms/MethodCard';
import { useNavigation } from '@/contexts/NavigationContext';
import { useToast } from '@/contexts/ToastContext';
import { deleteMethod, listMethods } from '@/lib/ipc';
import type { Method } from '@/types/model';

const sanitize = (str: string) =>
  str
    .toLowerCase()
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '');

export default function LibraryScreen() {
  const { goToMethod } = useNavigation();
  const { showToast } = useToast();
  const [q, setQ] = useState('');
  const [methods, setMethods] = useState<Method[]>([]);
  const lib = methods.filter((m) => sanitize(m.title).includes(sanitize(q)));

  useEffect(() => {
    listMethods().then(setMethods);
  }, []);

  const handleDelete = async (methodId: string) => {
    try {
      await deleteMethod(methodId);
      setMethods((prev) => prev.filter((m) => m.id !== methodId));
    } catch {
      showToast('Échec de la suppression de la méthode', 'error');
    }
  };

  return (
    <div className="scroll flex-1">
      <div className="m-0 mx-auto max-w-7xl p-[clamp(24px,4vw,48px)]">
        <div className="mb-7 flex flex-wrap items-end justify-between gap-5">
          <div>
            <div className="eyebrow mn-2">Bibliothèque</div>
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
            <MethodCard key={m.id} method={m} onOpen={goToMethod} onDelete={handleDelete} />
          ))}
        </div>
      </div>
    </div>
  );
}
