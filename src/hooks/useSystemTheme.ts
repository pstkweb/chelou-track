import { getCurrentWindow, type Theme } from '@tauri-apps/api/window';
import { useEffect, useState } from 'react';

export default function useSystemTheme() {
  const [theme, setTheme] = useState<'light' | 'dark'>('dark');

  useEffect(() => {
    const window = getCurrentWindow();
    const applyTheme = (theme: Theme | null) => setTheme(theme === 'light' ? 'light' : 'dark');

    window.theme().then(applyTheme);
    const unlisten = window.onThemeChanged((e) => applyTheme(e.payload));

    return () => {
      unlisten.then((fn) => fn());
    };
  }, []);

  useEffect(() => {
    document.documentElement.dataset['mode'] = theme;
  }, [theme]);

  return theme;
}
