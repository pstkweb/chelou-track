import { createContext, useCallback, useContext, useEffect, useMemo, useReducer } from 'react';
import type { Chapter } from '@/lib/method-view';
import type { Lesson, Method, TabSet } from '@/types/model';
import { useBreadcrumb } from './BreadcrumbContext';

export type Screen =
  | { id: 'library' }
  | { id: 'method'; method: Method }
  | { id: 'player'; method: Method; lesson: Lesson; chapter: Chapter }
  | { id: 'tab'; method: Method; lesson: Lesson; chapter: Chapter; tab: TabSet }
  | { id: 'documents'; method: Method };

type NavAction =
  | { type: 'library' }
  | { type: 'method'; method: Method }
  | { type: 'player'; lesson: Lesson; chapter: Chapter }
  | { type: 'tab'; lesson: Lesson; chapter: Chapter; tab: TabSet }
  | { type: 'documents'; method: Method };

function navReducer(screen: Screen, action: NavAction): Screen {
  switch (action.type) {
    case 'library':
      return { id: 'library' };
    case 'method':
      return { id: 'method', method: action.method };
    case 'player':
      if (screen.id === 'method' || screen.id === 'player' || screen.id === 'tab') {
        return {
          id: 'player',
          method: screen.method,
          lesson: action.lesson,
          chapter: action.chapter,
        };
      }
      return screen;
    case 'tab':
      if (screen.id === 'tab' || screen.id === 'player') {
        return {
          id: 'tab',
          method: screen.method,
          lesson: action.lesson,
          chapter: action.chapter,
          tab: action.tab,
        };
      }
      return screen;
    case 'documents':
      return { id: 'documents', method: action.method };
  }
}

interface NavigationContextValue {
  screen: Screen;
  goToLibrary: () => void;
  goToMethod: (method: Method) => void;
  openLesson: (lesson: Lesson, chapter: Chapter) => void;
  openTab: (lesson: Lesson, chapter: Chapter, tab: TabSet) => void;
  listDocuments: (method: Method) => void;
}

const NavigationContext = createContext<NavigationContextValue | undefined>(undefined);

export function NavigationProvider({ children }: { children: React.ReactNode }) {
  const [screen, dispatch] = useReducer(navReducer, { id: 'library' });
  const { dispatch: dispatchBreadcrumb } = useBreadcrumb();

  const goToLibrary = useCallback(() => dispatch({ type: 'library' }), []);
  const goToMethod = useCallback((method: Method) => dispatch({ type: 'method', method }), []);
  const openLesson = useCallback(
    (lesson: Lesson, chapter: Chapter) => dispatch({ type: 'player', lesson, chapter }),
    [],
  );
  const openTab = useCallback(
    (lesson: Lesson, chapter: Chapter, tab: TabSet) =>
      dispatch({ type: 'tab', lesson, chapter, tab }),
    [],
  );
  const listDocuments = useCallback(
    (method: Method) => dispatch({ type: 'documents', method }),
    [],
  );

  useEffect(() => {
    if (screen.id === 'library') {
      dispatchBreadcrumb({ type: 'replace', payload: [{ label: 'Bibliothèque' }] });
    } else if (screen.id === 'method') {
      dispatchBreadcrumb({
        type: 'replace',
        payload: [{ label: 'Bibliothèque', onClick: goToLibrary }, { label: screen.method.title }],
      });
    } else if (screen.id === 'documents') {
      dispatchBreadcrumb({
        type: 'replace',
        payload: [
          { label: 'Bibliothèque', onClick: goToLibrary },
          { label: screen.method.title, onClick: () => goToMethod(screen.method) },
          { label: 'Documents' },
        ],
      });
    } else if (screen.id === 'tab') {
      dispatchBreadcrumb({
        type: 'replace',
        payload: [
          { label: 'Bibliothèque', onClick: goToLibrary },
          { label: screen.method.title, onClick: () => goToMethod(screen.method) },
          { label: screen.lesson.title, onClick: () => openLesson(screen.lesson, screen.chapter) },
          { label: screen.tab.title },
        ],
      });
    } else {
      dispatchBreadcrumb({
        type: 'replace',
        payload: [
          { label: 'Bibliothèque', onClick: goToLibrary },
          { label: screen.method.title, onClick: () => goToMethod(screen.method) },
          { label: screen.lesson.title },
        ],
      });
    }
  }, [screen, dispatchBreadcrumb, goToLibrary, goToMethod, openLesson]);

  const value = useMemo(
    () => ({ screen, goToLibrary, goToMethod, openLesson, openTab, listDocuments }),
    [screen, goToLibrary, goToMethod, openLesson, openTab, listDocuments],
  );

  return <NavigationContext.Provider value={value}>{children}</NavigationContext.Provider>;
}

export function useNavigation() {
  const ctx = useContext(NavigationContext);
  if (ctx === undefined) {
    throw new Error('useNavigation must be used within a NavigationProvider');
  }
  return ctx;
}
