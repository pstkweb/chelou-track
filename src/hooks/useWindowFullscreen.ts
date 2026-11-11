import { getCurrentWindow } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';

/**
 * Plein écran de la fenêtre de l'app (celui du système, pas celui d'un élément du DOM — cf.
 * `useFullscreen`). `enabled` à false : pas d'abonnement, toujours false.
 */
export default function useWindowFullscreen(enabled = true) {
  const [fullscreen, setFullscreen] = useState(false);

  useEffect(() => {
    if (!enabled) return;
    const win = getCurrentWindow();
    win.isFullscreen().then(setFullscreen);
    const unlisten = win.onResized(async () => {
      setFullscreen(await win.isFullscreen());
    });
    return () => {
      unlisten.then((fn) => fn());
    };
  }, [enabled]);

  return fullscreen;
}
