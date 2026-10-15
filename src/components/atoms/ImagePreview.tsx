import { useEffect, useRef, useState } from 'react';
import { docUrl, fetchStream } from '@/lib/stream';
import { getCachedThumbnail, setCachedThumbnail } from '@/lib/thumbnail-cache';
import type { DocumentRef, Provider } from '@/types/model';

type ImagePreviewProps = {
  document: DocumentRef;
  provider: Provider;
};

export default function ImagePreview({ document, provider }: ImagePreviewProps) {
  const [src, setSrc] = useState<string | undefined>();
  const [failed, setFailed] = useState(false);
  const objectUrlRef = useRef<string | undefined>(undefined);

  useEffect(() => {
    let cancelled = false;
    setFailed(false);

    const showBlob = (blob: Blob) => {
      if (cancelled) return;
      const url = URL.createObjectURL(blob);
      objectUrlRef.current = url;
      setSrc(url);
    };

    getCachedThumbnail(provider, document.file.fileId)
      .then((cached) => {
        if (cached) return showBlob(cached);

        return fetchStream(docUrl(provider, document.file.fileId))
          .then((res) => res.blob())
          .then((blob) => {
            setCachedThumbnail(provider, document.file.fileId, blob);
            showBlob(blob);
          });
      })
      .catch((err) => {
        console.warn(`image preview failed for ${document.file.name}`, err);
        if (!cancelled) setFailed(true);
      });

    return () => {
      cancelled = true;
      if (objectUrlRef.current) URL.revokeObjectURL(objectUrlRef.current);
    };
  }, [document.file.fileId, document.file.name, provider]);

  if (failed) {
    return <span className="w-full text-center text-fg3 text-xs">Aperçu indisponible</span>;
  }

  return <img src={src} alt={document.title} />;
}
