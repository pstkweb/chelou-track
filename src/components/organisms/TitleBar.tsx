import { platform } from '@tauri-apps/plugin-os';
import { Guitar, LogOut } from 'lucide-react';
import { useEffect, useState } from 'react';
import Chip from '@/components/atoms/Chip';
import IconButton from '@/components/atoms/IconButton';
import WindowControls from '@/components/atoms/WindowControls';
import Breadcrumb from '@/components/molecules/Breadcrumb';
import { useBreadcrumb } from '@/contexts/BreadcrumbContext';
import { PROVIDERS } from '@/lib/providers';
import type { Provider } from '@/types/model';

type TitleBarProps = {
  connectedProvider: Provider | undefined;
  onLogout: () => void;
};

export default function TitleBar({ connectedProvider, onLogout }: TitleBarProps) {
  const [isMac, setIsMac] = useState<boolean>(false);
  const { items: breadcrumb } = useBreadcrumb();

  useEffect(() => {
    setIsMac(platform() === 'macos');
  }, []);

  return (
    <div className="titlebar" data-tauri-drag-region>
      {isMac && <WindowControls />}
      <div className="ml-1.5 flex items-center gap-2">
        <div className="flex size-5 items-center justify-center rounded bg-accent text-accentink">
          <Guitar size={12} />
        </div>
        <span className="tb-title">Chelou&nbsp;Track</span>
      </div>
      {breadcrumb.length > 0 && <Breadcrumb items={breadcrumb} />}
      <div className="tb-spacer" />
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
