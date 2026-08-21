import { Cloud, type LucideIcon } from 'lucide-react';
import type { Provider } from '@/types/model';

export const PROVIDERS: Record<Provider, { label: string; icon: LucideIcon }> = {
  pcloud: { label: 'pCloud', icon: Cloud },
  gdrive: { label: 'Google Drive', icon: Cloud },
  dropbox: { label: 'Dropbox', icon: Cloud },
};
