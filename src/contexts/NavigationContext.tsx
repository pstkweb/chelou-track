import { createContext, useCallback, useContext, useEffect, useMemo, useReducer } from "react";
import type { Chapter } from "@/lib/method-view";
import type { Lesson, Method } from "@/types/model";
import { useBreadcrumb } from "./BreadcrumbContext";

export type MediaType = "video" | "tab";

export type Screen =
  | { id: "library" }
  | { id: "method"; method: Method }
  | { id: "player"; method: Method; lesson: Lesson; chapter: Chapter; mediaType: MediaType };

type NavAction =
  | { type: "library" }
  | { type: "method"; method: Method }
  | { type: "player"; lesson: Lesson; chapter: Chapter; mediaType: MediaType };

function navReducer(screen: Screen, action: NavAction): Screen {
  switch (action.type) {
    case "library":
      return { id: "library" };
    case "method":
      return { id: "method", method: action.method };
    case "player":
      if (screen.id === "method" || screen.id === "player") {
        return {
          id: "player",
          method: screen.method,
          lesson: action.lesson,
          chapter: action.chapter,
          mediaType: action.mediaType,
        };
      }
      return screen;
  }
}

interface NavigationContextValue {
  screen: Screen;
  goToLibrary: () => void;
  goToMethod: (method: Method) => void;
  openLesson: (lesson: Lesson, chapter: Chapter, mediaType: MediaType) => void;
}

const NavigationContext = createContext<NavigationContextValue | undefined>(undefined);

export function NavigationProvider({ children }: { children: React.ReactNode }) {
  const [screen, dispatch] = useReducer(navReducer, { id: "library" });
  const { dispatch: dispatchBreadcrumb } = useBreadcrumb();

  const goToLibrary = useCallback(() => dispatch({ type: "library" }), []);
  const goToMethod = useCallback((method: Method) => dispatch({ type: "method", method }), []);
  const openLesson = useCallback(
    (lesson: Lesson, chapter: Chapter, mediaType: MediaType) =>
      dispatch({ type: "player", lesson, chapter, mediaType }),
    [],
  );

  useEffect(() => {
    if (screen.id === "library") {
      dispatchBreadcrumb({ type: "replace", payload: [{ label: "Bibliothèque" }] });
    } else if (screen.id === "method") {
      dispatchBreadcrumb({
        type: "replace",
        payload: [{ label: "Bibliothèque", onClick: goToLibrary }, { label: screen.method.title }],
      });
    } else {
      dispatchBreadcrumb({
        type: "replace",
        payload: [
          { label: "Bibliothèque", onClick: goToLibrary },
          { label: screen.method.title, onClick: () => goToMethod(screen.method) },
          { label: screen.lesson.title },
        ],
      });
    }
  }, [screen, dispatchBreadcrumb, goToLibrary, goToMethod]);

  const value = useMemo(
    () => ({ screen, goToLibrary, goToMethod, openLesson }),
    [screen, goToLibrary, goToMethod, openLesson],
  );

  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
}

export function useNavigation() {
  const ctx = useContext(NavigationContext);
  if (ctx === undefined) {
    throw new Error("useNavigation must be used within a NavigationProvider");
  }
  return ctx;
}
