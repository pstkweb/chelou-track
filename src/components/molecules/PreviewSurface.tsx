import { docUrl } from '@/lib/stream';
import type { DocumentRef } from '@/types/model';
import PdfPreview from '../atoms/PdfPreview';

type PreviewSurfaceProps = {
  document: DocumentRef;
};

export default function PreviewSurface({ document }: PreviewSurfaceProps) {
  if (document.kind === 'pdf')
    return (
      <div className="flex size-full items-center justify-center bg-bg3">
        <PdfPreview document={document} />
      </div>
    );

  if (document.kind === 'image') {
    return (
      <div className="flex size-full items-center overflow-hidden bg-bg3">
        <img src={docUrl(document.file.fileId)} alt={document.title} />
      </div>
    );
  }
}
