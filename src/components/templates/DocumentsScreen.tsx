import { Maximize2 } from 'lucide-react';
import { useEffect, useState } from 'react';
import type { DocumentRef, Method } from '@/types/model';
import Badge from '../atoms/Badge';
import IconButton from '../atoms/IconButton';
import PreviewSurface from '../molecules/PreviewSurface';
import DocumentModal from '../organisms/DocumentModal';

type DocumentsScreenProps = {
  method: Method;
};

export default function DocumentsScreen({ method }: DocumentsScreenProps) {
  const [openedDoc, setOpenedDoc] = useState<DocumentRef | null>(null);
  const kindLabel = { pdf: 'PDF', image: 'Image', video: 'Vidéo' };

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpenedDoc(null);
    };
    window.addEventListener('keydown', onKey);
    return () => window.removeEventListener('keydown', onKey);
  }, []);

  return (
    <div className="scroll" style={{ flex: 1 }}>
      <div className="mx-auto my-0 max-w-360 p-[clamp(24px,3.5vw,44px)]">
        <div className="eyebrow mt-1.5">{method.title} · Documents</div>
        <h1 className="display m-0 mb-1.5 text-[clamp(30px,3.5vw,46px)]">Dossier documents</h1>
        <p className="m-0 mb-6 text-fg3 text-sm">Double-clique sur un document pour l'ouvrir.</p>

        <div className="grid grid-cols-[repeat(auto-fill,minmax(200px,1fr))] gap-(--gap)">
          {method.documents.map((doc) => (
            <div
              key={doc.file.fileId}
              className="card transform-none cursor-pointer overflow-hidden border-border p-0 transition-[transform,border-color] duration-150 hover:-translate-y-0.5 hover:border-border2"
              onClick={() => setOpenedDoc(doc)}
              onKeyUp={() => {}}
              title="Double-clic pour ouvrir"
            >
              <div className="relative aspect-4/3">
                <PreviewSurface document={doc} />
                <Badge className="absolute top-2 left-2 bg-[#6D6E6F] text-white dark:bg-[#060709]!">
                  {kindLabel[doc.kind]}
                </Badge>
              </div>
              <div className="flex items-center gap-2 px-3.5 py-2.5">
                <div className="min-w-0 flex-1">
                  <div className="overflow-hidden text-ellipsis whitespace-nowrap font-semibold text-sm/snug">
                    {doc.title}
                  </div>
                </div>
                <IconButton
                  className="size-7"
                  title="Ouvrir"
                  onClick={(e) => {
                    e.stopPropagation();
                    setOpenedDoc(doc);
                  }}
                >
                  <Maximize2 size={16} />
                </IconButton>
              </div>
            </div>
          ))}
        </div>
      </div>

      {openedDoc && (
        <DocumentModal
          document={openedDoc}
          label={kindLabel[openedDoc.kind]}
          onClose={() => setOpenedDoc(null)}
        />
      )}
    </div>
  );
}
