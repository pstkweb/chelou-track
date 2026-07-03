import { createContext, type Dispatch, useContext, useMemo, useReducer } from 'react';

export type BreadcrumbItem = {
  label: string;
  onClick?: () => void;
};

type AddItemAction = {
  type: 'add';
  payload: BreadcrumbItem;
};

type ClearAction = {
  type: 'clear';
};

type ReplaceAction = {
  type: 'replace';
  payload: BreadcrumbItem[];
};

type BreadcrumbAction = AddItemAction | ClearAction | ReplaceAction;

interface BreadcrumbContextValue {
  items: BreadcrumbItem[];
  dispatch: Dispatch<BreadcrumbAction>;
}

const BreadcrumbContext = createContext<BreadcrumbContextValue | undefined>(undefined);

function breadcrumbReducer(items: BreadcrumbItem[], action: BreadcrumbAction) {
  switch (action.type) {
    case 'add':
      return [...items, action.payload];
    case 'replace':
      return [...action.payload];
    case 'clear':
      return [];
  }
}

export function BreadcrumbProvider({ children }: { children: React.ReactNode }) {
  const [items, dispatch] = useReducer(breadcrumbReducer, []);
  const contextValue = useMemo(() => ({ items, dispatch }), [items]);

  return <BreadcrumbContext.Provider value={contextValue}>{children}</BreadcrumbContext.Provider>;
}

export function useBreadcrumb() {
  const context = useContext(BreadcrumbContext);

  if (context === undefined) {
    throw new Error('useBreadcrumb must be used within a BreadcrumbProvider');
  }

  return context;
}
