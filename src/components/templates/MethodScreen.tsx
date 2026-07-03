import { Folder } from 'lucide-react';
import { useEffect, useState } from 'react';
import Chip from '@/components/atoms/Chip';
import MethodHero from '@/components/organisms/MethodHero';
import SidebarNav from '@/components/organisms/SidebarNav';
import useMethodView from '@/hooks/useMethodView';
import { listMethods } from '@/lib/ipc';
import type { Method } from '@/types/model';

type MethodScreenProps = {
  method: Method;
};

export default function MethodScreen({ method: initial }: MethodScreenProps) {
  const [method, setMethod] = useState(initial);

  useEffect(() => {
    listMethods().then((methods) => {
      const fresh = methods.find((m) => m.id === initial.id);
      if (fresh) setMethod(fresh);
    });
  }, [initial.id]);

  const { chapters, stats, currentChapter, currentLesson } = useMethodView(method);

  return (
    <div className="scroll flex-1">
      <div className="mx-auto my-0 w-[min(1440px,100%)] p-[clamp(24px,3.5vw,44px)]">
        <MethodHero title={method.title} methodStats={stats} currentLesson={currentLesson} />

        <div className="mx-0 mt-9 mb-4 flex items-center justify-between">
          <h2 className="display m-0 text-2xl">Chapitres</h2>
          <Chip as="button" className="cursor-pointer" onClick={() => {}}>
            <Folder size={15} /> Dossier documents · {method.documents.length}
          </Chip>
        </div>

        <SidebarNav chapters={chapters} currentChapterId={currentChapter?.id} />
      </div>
    </div>
  );
}
