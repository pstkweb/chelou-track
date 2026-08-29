import { useEffect, useRef, useState } from 'react';
import { docUrl } from '@/lib/stream';
import { getCachedThumbnail, setCachedThumbnail } from '@/lib/thumbnail-cache';
import type { DocumentRef, Provider } from '@/types/model';

type ImagePreviewProps = {
  document: DocumentRef;
  provider: Provider;
};

export default function ImagePreview({ document, provider }: ImagePreviewProps) {
  const [src, setSrc] = useState<string | undefined>();
  const objectUrlRef = useRef<string | undefined>(undefined);

  useEffect(() => {
    let cancelled = false;

    const showBlob = (blob: Blob) => {
      if (cancelled) return;
      const url = URL.createObjectURL(blob);
      objectUrlRef.current = url;
      setSrc(url);
    };

    getCachedThumbnail(provider, document.file.fileId)
      .then((cached) => {
        if (cached) return showBlob(cached);

        return fetch(docUrl(provider, document.file.fileId))
          .then((res) => res.blob())
          .then((blob) => {
            setCachedThumbnail(provider, document.file.fileId, blob);
            showBlob(blob);
          });
      })
      .catch(() => {});

    return () => {
      cancelled = true;
      if (objectUrlRef.current) URL.revokeObjectURL(objectUrlRef.current);
    };
  }, [document.file.fileId, provider]);

  return <img src={src} alt={document.title} />;
}
