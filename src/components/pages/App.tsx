import { useEffect, useState } from "react";
import TitleBar from "@/components/organisms/TitleBar";
import ConnectScreen from "@/components/templates/ConnectScreen";
import LessonScreen from "@/components/templates/LessonScreen";
import LibraryScreen from "@/components/templates/LibraryScreen";
import MethodScreen from "@/components/templates/MethodScreen";
import { BreadcrumbProvider } from "@/contexts/BreadcrumbContext";
import { NavigationProvider, useNavigation } from "@/contexts/NavigationContext";
import { getAuthStatus, listMethods, markLessonSeen } from "@/lib/ipc";

type AppState = "loading" | "no-auth" | "no-methods" | "ready";

function AuthenticatedView() {
  const { screen } = useNavigation();

  if (screen.id === "method") return <MethodScreen method={screen.method} />;
  if (screen.id === "player") {
    const handleVideoEnded = () => {
      markLessonSeen(screen.method.id, screen.lesson.id);
    };

    return (
      <LessonScreen
        key={screen.lesson.id}
        chapter={screen.chapter}
        method={screen.method}
        lesson={screen.lesson}
        onVideoEnd={handleVideoEnded}
      />
    );
  }

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
