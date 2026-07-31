import { Plus } from 'lucide-react';
import { useEffect, useState } from 'react';
import SearchField from '@/components/molecules/SearchField';
import MethodCard from '@/components/organisms/MethodCard';
import { useLibrary } from '@/contexts/LibraryContext';
import { useNavigation } from '@/contexts/NavigationContext';
import { useToast } from '@/contexts/ToastContext';
import type { Method } from '@/types/model';
import Button from '../atoms/Button';
import IconButton from '../atoms/IconButton';
import FolderPicker from '../organisms/FolderPicker';

const sanitize = (str: string) =>
  str
    .toLowerCase()
    .normalize('NFD')
    .replace(/\p{Diacritic}/gu, '');

export default function LibraryScreen() {
  const { goToMethod } = useNavigation();
  const { showToast } = useToast();
  const { methods, refresh, remove } = useLibrary();
  const [q, setQ] = useState('');
  const [isAdding, setIsAdding] = useState(false);
  const library = methods.filter((m) => sanitize(m.title).includes(sanitize(q)));

  useEffect(() => {
    refresh();
  }, [refresh]);

  const handleDelete = async (method: Method) => {
    try {
      await remove(method);
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
          <div className="flex items-center gap-2.5">
            <SearchField
              value={q}
              onChange={(e) => setQ(e.target.value)}
              placeholder="Rechercher une méthode…"
            />
            <Button variant="primary" onClick={() => setIsAdding(true)}>
              <Plus size={16} /> Ajouter une méthode
            </Button>
          </div>
        </div>

        <div className="grid grid-cols-[repeat(auto-fill,minmax(300px,1fr))] gap-(--gap)">
          {library.map((m) => (
            <MethodCard key={m.id} method={m} onOpen={goToMethod} onDelete={handleDelete} />
          ))}
        </div>

        {library.length === 0 && (
          <div className="px-0 py-15 text-center text-fg3 text-sm">
            Aucune méthode dans ta bibliothèque. Ajoute-en une pour commencer.
          </div>
        )}
      </div>

      {isAdding && (
        <div
          role="dialog"
          aria-modal
          onClick={() => setIsAdding(false)}
          className="fixed inset-0 z-50 flex items-center justify-center bg-black/55 p-6 backdrop-blur-xs"
          onKeyDown={() => {}}
        >
          <div onClick={(e) => e.stopPropagation()} className="relative" onKeyDown={() => {}}>
            <IconButton
              onClick={() => setIsAdding(false)}
              title="Fermer"
              className="absolute -top-4 -right-4 z-1 border border-border bg-bg2"
            >
              X
            </IconButton>
            <FolderPicker
              title="Quel dossier de méthode importer ?"
              onConnected={() => setIsAdding(false)}
            />
          </div>
        </div>
      )}
    </div>
  );
}
