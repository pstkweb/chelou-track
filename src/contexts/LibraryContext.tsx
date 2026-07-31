import { createContext, type Dispatch, useContext, useMemo, useReducer } from 'react';
import { deleteMethod, listMethods, saveMethod } from '@/lib/ipc';
import type { Method } from '@/types/model';

interface LibraryContextValue {
  methods: Method[];
  dispatch: Dispatch<LibraryAction>;
  remove: (method: Method) => Promise<void>;
  add: (method: Method) => Promise<void>;
  refresh: () => Promise<void>;
}

type LibraryAction =
  | { type: 'add'; method: Method }
  | { type: 'delete'; method: Method }
  | { type: 'set'; methods: Method[] };

const LibraryContext = createContext<LibraryContextValue | undefined>(undefined);

function libraryReducer(library: Method[], action: LibraryAction) {
  switch (action.type) {
    case 'add':
      return library.some((m) => m.id === action.method.id) ? library : [...library, action.method];
    case 'delete':
      return library.filter((m) => m.id !== action.method.id);
    case 'set':
      return action.methods;
  }
}

export function LibraryProvider({ children }: { children: React.ReactNode }) {
  const [methods, dispatch] = useReducer(libraryReducer, []);

  async function remove(method: Method) {
    await deleteMethod(method.id);

    dispatch({ type: 'delete', method });
  }

  async function add(method: Method) {
    await saveMethod(method);

    dispatch({ type: 'add', method });
  }

  async function refresh() {
    const methods = await listMethods();

    dispatch({ type: 'set', methods });
  }

  // biome-ignore lint/correctness/useExhaustiveDependencies: stable local functions
  const contextValue = useMemo(() => ({ methods, dispatch, remove, add, refresh }), [methods]);

  return <LibraryContext.Provider value={contextValue}>{children}</LibraryContext.Provider>;
}

export function useLibrary() {
  const context = useContext(LibraryContext);

  if (context === undefined) {
    throw new Error('useLibrary must be used within a LibraryProvider');
  }

  return context;
}
