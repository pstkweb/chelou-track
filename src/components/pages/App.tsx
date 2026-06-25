import { useEffect, useState } from "react";
import { getAuthStatus, listMethods } from "../../lib/ipc";
import TitleBar from "../organisms/TitleBar";
import ConnectScreen from "../templates/ConnectScreen";
import LibraryScreen from "../templates/LibraryScreen";

type AppState = "loading" | "no-auth" | "no-methods" | "ready";

export default function App() {
  const [state, setState] = useState<AppState>("loading");

  useEffect(() => {
    getAuthStatus().then((authed) => {
      if (!authed) {
        setState("no-auth");

        return;
      }

      return listMethods().then((methods) => {
        setState(methods.length > 0 ? "ready" : "no-methods");
      });
    });
  }, []);

  if (state === "loading") return null;

  return (
    <div className="app-window">
      <TitleBar connected={["no-methods", "ready"].includes(state)} crumbs={[]} />

      <div className="app-body">
        {state === "ready" ? (
          <LibraryScreen onOpen={() => {}} />
        ) : (
          <ConnectScreen
            startAtFolder={state === "no-methods"}
            onConnected={() => setState("ready")}
          />
        )}
      </div>
    </div>
  );
}
