import { useCallback, useEffect, useRef } from 'react';
import { useLibrary } from '@/contexts/LibraryContext';

export default function useTimecodeResume(methodId: string, lessonId: string, delay = 5_000) {
  const { setResume } = useLibrary();
  const timerRef = useRef<number | null>(null);
  const pendingMsRef = useRef<number | null>(null);

  const flush = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }

    if (pendingMsRef.current !== null) {
      void setResume(methodId, lessonId, pendingMsRef.current);
      pendingMsRef.current = null;
    }
  }, [methodId, lessonId, setResume]);

  const record = useCallback(
    (currentTimeSeconds: number) => {
      pendingMsRef.current = currentTimeSeconds * 1000;

      if (timerRef.current !== null) clearTimeout(timerRef.current);

      timerRef.current = window.setTimeout(flush, delay);
    },
    [flush, delay],
  );

  useEffect(() => flush, [flush]);

  useEffect(() => {
    window.addEventListener('beforeunload', flush);
    return () => window.removeEventListener('beforeunload', flush);
  }, [flush]);

  const cancel = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }
    pendingMsRef.current = null;
  }, []);

  return { record, cancel };
}
