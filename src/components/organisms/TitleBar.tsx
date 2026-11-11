import { getVersion } from '@tauri-apps/api/app';
import { platform } from '@tauri-apps/plugin-os';
import { Guitar, LogOut } from 'lucide-react';
import { useEffect, useState } from 'react';
import Chip from '@/components/atoms/Chip';
import IconButton from '@/components/atoms/IconButton';
import WindowControls from '@/components/atoms/WindowControls';
import Breadcrumb from '@/components/molecules/Breadcrumb';
import GuitarControl from '@/components/organisms/GuitarControl';
import { useBreadcrumb } from '@/contexts/BreadcrumbContext';
import useWindowFullscreen from '@/hooks/useWindowFullscreen';
import { PROVIDERS } from '@/lib/providers';
import type { Provider } from '@/types/model';

type TitleBarProps = {
  connectedProvider: Provider | undefined;
  onLogout: () => void;
};

// macOS : boutons natifs du système (feux tricolores) superposés à la barre, à gauche
// (`tauri.macos.conf.json`, `titleBarStyle: "Overlay"`). Ailleurs : `WindowControls`, à droite.
const isMac = platform() === 'macos';

export default function TitleBar({ connectedProvider, onLogout }: TitleBarProps) {
  const [version, setVersion] = useState('');
  const { items: breadcrumb } = useBreadcrumb();
  // Plein écran macOS : les feux tricolores disparaissent avec la barre du système.
  const fullscreen = useWindowFullscreen(isMac);

  useEffect(() => {
    getVersion().then(setVersion);
  }, []);

  return (
    <div className="titlebar" data-tauri-drag-region>
      {/* Place des feux tricolores (posés par `src-tauri/src/titlebar.rs`, à 14 px du bord) ;
          espaceur plutôt que padding : `.titlebar` fixe déjà le sien. */}
      {isMac && !fullscreen && <div className="w-16 flex-none" data-tauri-drag-region />}
      <div className="ml-1.5 flex items-center gap-2">
        <div className="flex size-5 items-center justify-center rounded bg-accent text-accentink">
          <Guitar size={12} />
        </div>
        <span className="tb-title">Chelou&nbsp;Track v{version || '??'}</span>
      </div>
      {breadcrumb.length > 0 && <Breadcrumb items={breadcrumb} />}
      <div className="tb-spacer" />
      <GuitarControl />
      {connectedProvider && (
        <>
          <Chip className="h-6 text-xs">
            <span className="size-2 flex-initial rounded-full bg-done" />{' '}
            {PROVIDERS[connectedProvider].label}
          </Chip>
          <IconButton onClick={onLogout} title="Se déconnecter" className="size-6">
            <LogOut size={13} />
          </IconButton>
        </>
      )}
      {!isMac && <WindowControls />}
    </div>
  );
}
