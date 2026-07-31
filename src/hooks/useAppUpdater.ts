import { relaunch } from '@tauri-apps/plugin-process';
import { check } from '@tauri-apps/plugin-updater';
import { useEffect } from 'react';
import { useToast } from '@/contexts/ToastContext';

export default function useAppUpdater() {
  const { showToast } = useToast();

  useEffect(() => {
    (async function checkUpdates() {
      try {
        const update = await check();

        if (update) {
          console.log(
            `Found update ${update.version} from ${update.date} with notes ${update.body}`,
          );
          showToast(
            "Une nouvelle version de l'application existe, elle va être installée puis l'application va redémarrer.",
          );

          await update.downloadAndInstall();
          await relaunch();
        }
      } catch (err) {
        console.error('Unable to check or update app', err);
      }
    })();
  }, [showToast]);
}
