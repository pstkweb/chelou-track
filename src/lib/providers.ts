import { type IconType, SiDropbox, SiGoogledrive } from '@icons-pack/react-simple-icons';
import { Cloud, type LucideIcon } from 'lucide-react';
import type { Provider } from '@/types/model';

export const PROVIDERS: Record<
  Provider,
  { label: string; rootId: string; icon: LucideIcon | IconType; color: string }
> = {
  pcloud: { label: 'pCloud', rootId: '0', icon: Cloud, color: '#17bed0' },
  gdrive: { label: 'Google Drive', rootId: '/', icon: SiGoogledrive, color: '#1a73e8' },
  dropbox: { label: 'Dropbox', rootId: '', icon: SiDropbox, color: '#0061fe' },
};
