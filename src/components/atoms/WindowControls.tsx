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
    <fieldset className="win-ctl" aria-label="Contrôles de la fenêtre">
      <button type="button" aria-label="Réduire" title="Réduire" onClick={handleMinimize}>
        <Minus size={15} />
      </button>
      <button
        type="button"
        aria-label={isMaximized ? "Restaurer" : "Agrandir"}
        title={isMaximized ? "Restaurer" : "Agrandir"}
        onClick={toggleMaximize}
      >
        {isMaximized ? <Copy size={15} /> : <Square size={15} />}
      </button>
      <button
        className="close"
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
