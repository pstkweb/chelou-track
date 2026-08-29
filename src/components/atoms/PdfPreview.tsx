import { useEffect, useRef } from 'react';
import { loadPdfDocument } from '@/lib/pdf';
import { getCachedThumbnail, setCachedThumbnail } from '@/lib/thumbnail-cache';
import type { DocumentRef, Provider } from '@/types/model';

type PdfPreviewProps = {
  document: DocumentRef;
  provider: Provider;
};

const THUMBNAIL_WIDTH = 400;

export default function PdfPreview({ document, provider }: PdfPreviewProps) {
  const canvasRef = useRef<HTMLCanvasElement | null>(null);

  useEffect(() => {
    let cancelled = false;

    const drawBlob = async (blob: Blob) => {
      const bitmap = await createImageBitmap(blob);
      const canvas = canvasRef.current;
      const context = canvas?.getContext('2d');
      if (cancelled || !canvas || !context) return;

      canvas.width = bitmap.width;
      canvas.height = bitmap.height;
      context.drawImage(bitmap, 0, 0);
    };

    // Re-fetching and re-rendering the whole PDF just for a 400px-wide thumbnail is wasteful —
    // cache the rendered result (see thumbnail-cache.ts) and only pay that cost once per file.
    const renderAndCache = async () => {
      const pdf = await loadPdfDocument(provider, document.file.fileId);
      const page = await pdf.getPage(1);
      const canvas = canvasRef.current;
      const context = canvas?.getContext('2d');
      if (cancelled || !canvas || !context) return;

      // Render at devicePixelRatio so the canvas stays crisp once CSS scales it down.
      const dpr = window.devicePixelRatio || 1;
      const baseViewport = page.getViewport({ scale: 1 });
      const viewport = page.getViewport({ scale: (THUMBNAIL_WIDTH / baseViewport.width) * dpr });
      canvas.width = viewport.width;
      canvas.height = viewport.height;

      await page.render({ canvasContext: context, viewport }).promise;

      canvas.toBlob((blob) => {
        if (blob) setCachedThumbnail(provider, document.file.fileId, blob);
      });
    };

    getCachedThumbnail(provider, document.file.fileId)
      .then((cached) => (cached ? drawBlob(cached) : renderAndCache()))
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [document.file.fileId, provider]);

  return <canvas ref={canvasRef} className="max-h-full max-w-full" />;
}
