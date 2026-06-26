import { platform } from "@tauri-apps/plugin-os";
import { Guitar } from "lucide-react";
import { useEffect, useState } from "react";
import { useBreadcrumb } from "../../contexts/BreadcrumbContext";
import Chip from "../atoms/Chip";
import WindowControls from "../atoms/WindowControls";
import Breadcrumb from "../molecules/Breadcrumb";

type TitleBarProps = {
  connected: boolean;
};

export default function TitleBar({ connected }: TitleBarProps) {
  const [isMac, setIsMac] = useState<boolean>(false);
  const {items: breadcrumb} = useBreadcrumb();

  useEffect(() => {
    setIsMac(platform() === "macos");
  }, []);

  return (
    <div
      className="relative z-10 flex h-9 flex-initial select-none items-center gap-2.5 border-border border-b bg-bg2 px-3.5 py-0"
      data-tauri-drag-region
    >
      {isMac && <WindowControls />}
      <div className="ml-1.5 flex items-center gap-2">
        <div className="flex size-5 items-center justify-center rounded bg-accent text-accentink">
          <Guitar size={12} />
        </div>
        <span className="font-medium text-fg2 text-xs tracking-wide">Chelou&nbsp;Track</span>
      </div>
      {breadcrumb.length > 0 && <Breadcrumb items={breadcrumb} />}
      <div className="flex-1" />
      {connected && (
        <Chip className="h-6 text-xs">
          <span className="size-2 flex-initial rounded-full bg-done" /> pCloud
        </Chip>
      )}
      {!isMac && <WindowControls />}
    </div>
  );
}
