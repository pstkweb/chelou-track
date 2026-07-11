import { Music } from 'lucide-react';
import cn from '@/lib/cn';
import type { BackingGroup, BackingTrack } from '@/types/model';
import Chip from '../atoms/Chip';

type BackingTrackPickerProps = {
  backingGroups: BackingGroup[];
  selectedGroup: BackingGroup | undefined;
  selectedTrack: BackingTrack | undefined;
  onGroupSelect: (group: BackingGroup) => void;
  onTrackSelect: (track: BackingTrack) => void;
};

export default function BackingTrackPicker({
  backingGroups,
  selectedGroup,
  selectedTrack,
  onGroupSelect,
  onTrackSelect,
}: BackingTrackPickerProps) {
  return (
    <div className="min-w-65 flex-1">
      <div className="eyebrow mb-1.5 text-xs">Backing track</div>
      <div className="mb-2 flex flex-wrap gap-1.5">
        {backingGroups.map((track) => (
          <Chip
            as="button"
            key={track.label}
            onClick={() => onGroupSelect(track)}
            className={cn(
              'h-8! cursor-pointer font-bold!',
              track.label === selectedGroup?.label && 'border-accent! bg-transparent! text-fg3!',
            )}
          >
            <Music size={13} /> {track.label}
          </Chip>
        ))}
      </div>
      <div className="eyebrow mb-1.5 text-xs">Tempo de travail</div>
      <div className="flex flex-wrap gap-1.5">
        {(selectedGroup?.tracks || []).map((track) => (
          <Chip
            as="button"
            key={track.audio.fileId}
            onClick={() => {
              onTrackSelect(track);
            }}
            className={cn(
              'h-8! cursor-pointer font-bold!',
              track.audio.fileId === selectedTrack?.audio.fileId &&
                'border-transparent! bg-accent! text-accentink!',
            )}
          >
            <b>{track.bpm}</b>
            <span className="font-medium opacity-70">BPM</span>
          </Chip>
        ))}
      </div>
    </div>
  );
}
