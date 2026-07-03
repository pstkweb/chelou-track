import { platform } from '@tauri-apps/plugin-os';
import { Guitar } from 'lucide-react';
import { useEffect, useState } from 'react';
import Chip from '@/components/atoms/Chip';
import WindowControls from '@/components/atoms/WindowControls';
import Breadcrumb from '@/components/molecules/Breadcrumb';
import { useBreadcrumb } from '@/contexts/BreadcrumbContext';

type TitleBarProps = {
  connected: boolean;
};

export default function TitleBar({ connected }: TitleBarProps) {
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
      {connected && (
        <Chip className="h-6 text-xs">
          <span className="size-2 flex-initial rounded-full bg-done" /> pCloud
        </Chip>
      )}
      {!isMac && <WindowControls />}
    </div>
  );
}
