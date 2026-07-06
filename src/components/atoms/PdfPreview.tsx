import { useEffect, useRef } from 'react';
import { loadPdfDocument } from '@/lib/pdf';
import type { DocumentRef } from '@/types/model';

type PdfPreviewProps = {
  document: DocumentRef;
};

const THUMBNAIL_WIDTH = 400;

export default function PdfPreview({ document }: PdfPreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    let cancelled = false;

    loadPdfDocument(document.file.fileId)
      .then((pdf) => pdf.getPage(1))
      .then((page) => {
        const canvas = canvasRef.current;
        const context = canvas?.getContext('2d');
        if (cancelled || !canvas || !context) return;

        // Render at devicePixelRatio so the canvas stays crisp once CSS scales it down.
        const dpr = window.devicePixelRatio || 1;
        const baseViewport = page.getViewport({ scale: 1 });
        const viewport = page.getViewport({ scale: (THUMBNAIL_WIDTH / baseViewport.width) * dpr });
        canvas.width = viewport.width;
        canvas.height = viewport.height;

        return page.render({ canvasContext: context, viewport }).promise;
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [document.file.fileId]);

  return <canvas ref={canvasRef} className="max-h-full max-w-full" />;
}
