import { useEffect, useState } from "react";
import TitleBar from "@/components/organisms/TitleBar";
import ConnectScreen from "@/components/templates/ConnectScreen";
import LibraryScreen from "@/components/templates/LibraryScreen";
import MethodScreen from "@/components/templates/MethodScreen";
import { BreadcrumbProvider } from "@/contexts/BreadcrumbContext";
import { NavigationProvider, useNavigation } from "@/contexts/NavigationContext";
import { getAuthStatus, listMethods } from "@/lib/ipc";

type AppState = "loading" | "no-auth" | "no-methods" | "ready";

function AuthenticatedView() {
  const { screen } = useNavigation();
  if (screen.id === "method") return <MethodScreen method={screen.method} />;
  // screen.id === "player" → à venir
  return <LibraryScreen />;
}

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
      <BreadcrumbProvider>
        <TitleBar connected={["no-methods", "ready"].includes(state)} />

        <div className="app-body">
          {state === "ready" ? (
            <NavigationProvider>
              <AuthenticatedView />
            </NavigationProvider>
          ) : (
            <ConnectScreen
              startAtFolder={state === "no-methods"}
              onConnected={() => setState("ready")}
            />
          )}
        </div>
      </BreadcrumbProvider>
    </div>
  );
}
