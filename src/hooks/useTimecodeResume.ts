import { useCallback, useEffect, useRef } from 'react';
import { updateLessonResume } from '@/lib/ipc';

export default function useTimecodeResume(methodId: string, lessonId: string, delay = 5_000) {
  const timerRef = useRef<number | null>(null);
  const pendingMsRef = useRef<number | null>(null);

  const flush = useCallback(() => {
    if (timerRef.current !== null) {
      clearTimeout(timerRef.current);
      timerRef.current = null;
    }

    if (pendingMsRef.current !== null) {
      void updateLessonResume(methodId, lessonId, pendingMsRef.current);
      pendingMsRef.current = null;
    }
  }, [methodId, lessonId]);

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
