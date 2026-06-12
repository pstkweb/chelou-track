import { Copy, Minus, Square, X } from "lucide-react";
import { getCurrentWindow } from '@tauri-apps/api/window'
import { useEffect, useState } from "react";

const appWindow = getCurrentWindow()

export default function WindowControls() {
    const [isMaximized, setIsMaximized] = useState<boolean>(false);

    useEffect(() => {
        appWindow.isMaximized().then(setIsMaximized);

        const unlisten = appWindow.onResized(async () => {
            setIsMaximized(await appWindow.isMaximized());
        });

        return () => {
            unlisten.then(fn => fn());
        };
    }, []);

    const toggleMaximize = () => {
        appWindow.toggleMaximize();
    };

    const handleClose = () => {
        appWindow.close();
    };

    const handleMinimize = () => {
        appWindow.minimize();
    };

    return (
        <div className="flex gap-0.5 [-webkit-app-region:no-drag]" role="group" aria-label="Contrôles de la fenêtre">
            <button className="w-8 h-6 p-0 border-0 bg-transparent rounded-sm text-fg3 flex items-center justify-center transition-colors hover:bg-chip hover:text-fg active:bg-border focus-visible:outline-2 focus-visible:outline-solid focus-visible:outline-accent" type="button" aria-label="Réduire" title="Réduire" onClick={handleMinimize}>
                <Minus size={15} />
            </button>
            <button className="w-8 h-6 p-0 border-0 bg-transparent rounded-sm text-fg3 flex items-center justify-center transition-colors hover:bg-chip hover:text-fg active:bg-border focus-visible:outline-2 focus-visible:outline-solid focus-visible:outline-accent" type="button" aria-label={isMaximized ? "Restaurer" : "Agrandir"} title={isMaximized ? "Restaurer" : "Agrandir"} onClick={toggleMaximize}>
                {isMaximized ? <Copy size={15} /> : <Square size={15} />}
            </button>
            <button className="w-8 h-6 p-0 border-0 bg-transparent rounded-sm text-fg3 flex items-center justify-center transition-colors hover:bg-red-600 hover:text-white active:bg-border focus-visible:outline-2 focus-visible:outline-solid focus-visible:outline-accent" type="button" aria-label="Fermer la fenêtre" title="Fermer" onClick={handleClose}>
                <X size={15} />
            </button>
        </div>
    );
}