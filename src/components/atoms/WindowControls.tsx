import { getCurrentWindow } from "@tauri-apps/api/window";
import { Copy, Minus, Square, X } from "lucide-react";
import { useEffect, useState } from "react";

const appWindow = getCurrentWindow();

export default function WindowControls() {
  const [isMaximized, setIsMaximized] = useState<boolean>(false);

  useEffect(() => {
    appWindow.theme().then((theme) => {
      let themeName = "dark";

      if (theme === "light") {
        themeName = "light";
      }

      document.documentElement.dataset["mode"] = themeName;
    });

    appWindow.isMaximized().then(setIsMaximized);

    const unlisten = appWindow.onResized(async () => {
      setIsMaximized(await appWindow.isMaximized());
    });

    return () => {
      unlisten.then((fn) => fn());
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
    <fieldset
      className="flex gap-0.5 [-webkit-app-region:no-drag]"
      aria-label="Contrôles de la fenêtre"
    >
      <button
        className="flex h-6 w-8 items-center justify-center rounded-sm border-0 bg-transparent p-0 text-fg3 transition-colors hover:bg-chip hover:text-fg focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-solid active:bg-border"
        type="button"
        aria-label="Réduire"
        title="Réduire"
        onClick={handleMinimize}
      >
        <Minus size={15} />
      </button>
      <button
        className="flex h-6 w-8 items-center justify-center rounded-sm border-0 bg-transparent p-0 text-fg3 transition-colors hover:bg-chip hover:text-fg focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-solid active:bg-border"
        type="button"
        aria-label={isMaximized ? "Restaurer" : "Agrandir"}
        title={isMaximized ? "Restaurer" : "Agrandir"}
        onClick={toggleMaximize}
      >
        {isMaximized ? <Copy size={15} /> : <Square size={15} />}
      </button>
      <button
        className="flex h-6 w-8 items-center justify-center rounded-sm border-0 bg-transparent p-0 text-fg3 transition-colors hover:bg-red-600 hover:text-white focus-visible:outline-2 focus-visible:outline-accent focus-visible:outline-solid active:bg-border"
        type="button"
        aria-label="Fermer la fenêtre"
        title="Fermer"
        onClick={handleClose}
      >
        <X size={15} />
      </button>
    </fieldset>
  );
}
