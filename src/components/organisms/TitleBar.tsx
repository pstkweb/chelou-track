import { Guitar } from "lucide-react";
import Chip from "../atoms/Chip";
import WindowControls from "../atoms/WindowControls";
import Breadcrumb from "../molecules/Breadcrumb";
import { useEffect, useState } from "react";
import { platform } from "@tauri-apps/plugin-os";

type TitleBarProps = {
    crumbs: { label: string, onClick?: () => void }[];
    connected: boolean;
};

export default function TitleBar({ crumbs, connected }: TitleBarProps) {
    const [isMac, setIsMac] = useState<boolean>(false);

    useEffect(() => {
        setIsMac(platform() === 'macos');
    }, []);

    return (
        <div className="flex-initial h-9 flex items-center gap-2.5 py-0 px-3.5 relative z-10 bg-bg2 border-b border-border select-none" data-tauri-drag-region>
            {isMac && <WindowControls />}
            <div className="flex items-center gap-2 ml-1.5">
                <div className="size-5 rounded bg-accent text-accentink flex items-center justify-center">
                    <Guitar size={12} />
                </div>
                <span className="text-xs font-medium text-fg2 tracking-wide">Chelou&nbsp;Track</span>
            </div>
            {crumbs.length > 0 && (
                <Breadcrumb items={crumbs} />
            )}
            <div className="flex-1" />
            {connected && (
                <Chip className="h-6 text-xs">
                    <span className="size-2 rounded-full flex-initial bg-done" /> pCloud
                </Chip>
            )}
            {!isMac && <WindowControls />}
        </div>
    );
}