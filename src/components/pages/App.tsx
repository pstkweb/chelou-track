import { useEffect, useState } from 'react';
import TitleBar from '@/components/organisms/TitleBar';
import ConnectScreen from '@/components/templates/ConnectScreen';
import LessonScreen from '@/components/templates/LessonScreen';
import LibraryScreen from '@/components/templates/LibraryScreen';
import MethodScreen from '@/components/templates/MethodScreen';
import { BreadcrumbProvider } from '@/contexts/BreadcrumbContext';
import { LibraryProvider, useLibrary } from '@/contexts/LibraryContext';
import { NavigationProvider, useNavigation } from '@/contexts/NavigationContext';
import { ToastProvider } from '@/contexts/ToastContext';
import useAppUpdater from '@/hooks/useAppUpdater';
import useSystemTheme from '@/hooks/useSystemTheme';
import { getAuthStatus, listMethods, logout } from '@/lib/ipc';
import type { Provider } from '@/types/model';
import DocumentsScreen from '../templates/DocumentsScreen';
import TabScreen from '../templates/TabScreen';

type AppState = 'loading' | 'no-auth' | 'no-methods' | 'ready';

function UpdateChecker() {
  useAppUpdater();

  return null;
}

function AuthenticatedView() {
  const { screen } = useNavigation();
  const { markSeen } = useLibrary();
  useSystemTheme();

  if (screen.id === 'method') {
    return <MethodScreen method={screen.method} />;
  }

  if (screen.id === 'player') {
    const handleVideoEnded = () => {
      markSeen(screen.method.id, screen.lesson.id);
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

  if (screen.id === 'tab') {
    return (
      <TabScreen
        key={screen.tab.id}
        chapter={screen.chapter}
        method={screen.method}
        lesson={screen.lesson}
        tab={screen.tab}
      />
    );
  }

  if (screen.id === 'documents') {
    return <DocumentsScreen method={screen.method} />;
  }

  return <LibraryScreen />;
}

export default function App() {
  const [state, setState] = useState<AppState>('loading');
  const [connectedProvider, setConnectedProvider] = useState<Provider | undefined>(undefined);

  useEffect(() => {
    getAuthStatus().then((provider) => {
      if (!provider) {
        setState('no-auth');

        return;
      }

      setConnectedProvider(provider);

      return listMethods().then((methods) => {
        setState(methods.length > 0 ? 'ready' : 'no-methods');
      });
    });
  }, []);

  const handleLogout = async () => {
    await logout();
    setConnectedProvider(undefined);
    setState('no-auth');
  };

  return (
    <div className="app-window">
      <ToastProvider>
        <UpdateChecker />
        <BreadcrumbProvider>
          {state !== 'loading' && (
            <LibraryProvider>
              <TitleBar connectedProvider={connectedProvider} onLogout={handleLogout} />

              <div className="app-body">
                {state === 'ready' ? (
                  <NavigationProvider>
                    <AuthenticatedView />
                  </NavigationProvider>
                ) : (
                  <ConnectScreen
                    startAtFolder={state === 'no-methods'}
                    provider={connectedProvider}
                    onConnected={(provider) => {
                      setConnectedProvider(provider);
                      setState('ready');
                    }}
                  />
                )}
              </div>
            </LibraryProvider>
          )}
        </BreadcrumbProvider>
      </ToastProvider>
    </div>
  );
}
