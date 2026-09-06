import { useEffect, useMemo, useState } from 'react';
import NativeFolderBrowser, { type Crumb } from '@/components/organisms/NativeFolderBrowser';
import ScanProgress from '@/components/organisms/ScanProgress';
import ScanReview from '@/components/organisms/ScanReview';
import type { FolderEntry } from '@/lib/ipc';
import { listFolder } from '@/lib/ipc';
import { PROVIDERS } from '@/lib/providers';
import type { Method, Provider } from '@/types/model';

type FolderPickerProps = {
  provider: Provider;
  title?: string;
  onConnected: (provider: Provider) => void;
};

type State = 'browse' | 'scan' | 'review';

export default function FolderPicker({
  provider,
  title = 'Où sont tes méthodes ?',
  onConnected,
}: FolderPickerProps) {
  const [state, setState] = useState<State>('browse');
  const [path, setPath] = useState<string>('/');
  // crumbs[0] is always root. The current folder is the last crumb.
  const [crumbs, setCrumbs] = useState<Crumb[]>([
    { id: PROVIDERS[provider].rootId, name: PROVIDERS[provider].label },
  ]);
  const [entries, setEntries] = useState<FolderEntry[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [revealedMethods, setRevealedMethods] = useState<Method[]>([]);

  const currentId = crumbs[crumbs.length - 1]?.id ?? '0';

  const pathFromCrumbs = useMemo(
    () =>
      `/${crumbs
        .map((c) => c.name)
        .slice(1)
        .join('/')}`,
    [crumbs],
  );

  useEffect(() => {
    let cancelled = false;

    setLoading(true);
    setError(null);

    listFolder(provider, currentId)
      .then((folders) => {
        if (!cancelled) {
          setEntries(folders);
          setPath(pathFromCrumbs);
          setLoading(false);
        }
      })
      .catch((err) => {
        if (!cancelled) {
          setError(String(err));
          setLoading(false);
        }
      });

    return () => {
      cancelled = true;
    };
  }, [provider, currentId, pathFromCrumbs]);

  const enter = (entry: FolderEntry) =>
    setCrumbs((prev) => [...prev, { id: entry.folderid, name: entry.name }]);

  const goTo = (index: number) => setCrumbs((prev) => prev.slice(0, index + 1));

  const handleScanDone = (methods: Method[]) => {
    setRevealedMethods(methods);
    setState('review');
  };

  if (state === 'scan') {
    return (
      <ScanProgress
        folderId={currentId}
        path={path}
        provider={provider}
        onDone={handleScanDone}
        onCancel={() => setState('browse')}
      />
    );
  }

  if (state === 'review') {
    return (
      <ScanReview
        foundMethods={revealedMethods}
        onImport={() => onConnected(provider)}
        onBack={() => setState('browse')}
      />
    );
  }

  return (
    <NativeFolderBrowser
      title={title}
      providerLabel={PROVIDERS[provider].label}
      ProviderIcon={PROVIDERS[provider].icon}
      crumbs={crumbs}
      entries={entries}
      loading={loading}
      error={error}
      onGoTo={goTo}
      onEnter={enter}
      onScan={() => setState('scan')}
    />
  );
}
