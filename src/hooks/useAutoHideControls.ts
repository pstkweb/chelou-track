import { useCallback, useEffect, useRef, useState } from 'react';

export default function useAutoHideControls(isPlaying: boolean) {
  const [areControlsVisible, setAreControlsVisible] = useState(true);
  const controlsTimer = useRef<number | null>(null);

  const resetControlsTimer = useCallback(() => {
    if (controlsTimer.current) {
      clearTimeout(controlsTimer.current);
    }
    setAreControlsVisible(true);
    if (isPlaying) {
      controlsTimer.current = window.setTimeout(() => {
        setAreControlsVisible(false);
      }, 2000);
    }
  }, [isPlaying]);

  useEffect(() => {
    resetControlsTimer();

    return () => {
      if (controlsTimer.current) {
        clearTimeout(controlsTimer.current);
      }
    };
  }, [resetControlsTimer]);

  return { areControlsVisible, resetControlsTimer };
}
