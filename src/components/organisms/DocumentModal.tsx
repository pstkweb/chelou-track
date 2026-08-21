import { Download } from 'lucide-react';
import { useToast } from '@/contexts/ToastContext';
import { docUrl } from '@/lib/stream';
import type { DocumentRef, Provider } from '@/types/model';
import Badge from '../atoms/Badge';
import Button from '../atoms/Button';
import IconButton from '../atoms/IconButton';

type DocumentModalProps = {
  document: DocumentRef;
  label: string;
  provider: Provider;
  onClose: () => void;
};

async function handleDownload(
  document: DocumentRef,
  provider: Provider,
  showToast: (message: string, variant?: 'success' | 'error') => void,
) {
  try {
    const res = await fetch(docUrl(provider, document.file.fileId));
    const blob = await res.blob();
    const url = URL.createObjectURL(blob);
    const link = window.document.createElement('a');
    link.href = url;
    link.download = document.file.name;
    link.click();
    URL.revokeObjectURL(url);
    showToast(`Téléchargé dans votre dossier Téléchargements : ${document.file.name}`);
  } catch {
    showToast(`Échec du téléchargement de ${document.file.name}`, 'error');
  }
}

export default function DocumentModal({ document, label, provider, onClose }: DocumentModalProps) {
  const { showToast } = useToast();

  return (
    <div
      onClick={onClose}
      className="animation-[fadeUp_.2s_var(--ease)] fixed inset-0 z-50 flex items-center justify-center bg-[rgba(0,0,0,.6)] p-8 backdrop-blur-xs"
      onKeyUp={() => {}}
    >
      <div
        className="card flex max-h-[92vh] max-w-[96vw] flex-col overflow-hidden"
        onClick={(e) => e.stopPropagation()}
        onKeyUp={() => {}}
      >
        <div className="flex items-center gap-3 border-border border-b px-4 py-3.5">
          <Badge className="text-accent">{label}</Badge>
          <div className="flex-1 font-bold text-sm">{document.title}</div>
          <Button onClick={() => handleDownload(document, provider, showToast)}>
            <Download size={16} /> Télécharger
          </Button>
          <IconButton onClick={onClose} className="border border-border">
            ✕
          </IconButton>
        </div>
        <div className="flex min-h-0 flex-1 items-center justify-center bg-bg3 p-8">
          {document.kind === 'pdf' && (
            <embed
              src={`${docUrl(provider, document.file.fileId)}#view=FitH`}
              type="application/pdf"
              className="aspect-[1/1.414] h-[calc(92vh-200px)]"
            />
          )}
          {document.kind === 'image' && (
            <img
              src={docUrl(provider, document.file.fileId)}
              alt={document.title}
              className="max-h-[calc(92vh-200px)] max-w-[calc(96vw-64px)] rounded-(--radius) bg-white"
            />
          )}
        </div>
      </div>
    </div>
  );
}
