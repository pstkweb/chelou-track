import type { LucideIcon } from 'lucide-react';

type MixSliderProps = {
  label: string;
  icon: LucideIcon;
  value: number;
  onChange: (value: number) => void;
};

export default function MixSlider({ label, icon: I, value, onChange }: MixSliderProps) {
  return (
    <div className="flex w-24 flex-col gap-1">
      <div className="flex items-center gap-1.5 text-fg3 text-xs">
        <I size={13} /> {label}
      </div>
      <input
        type="range"
        min="0"
        max="100"
        value={value}
        onChange={(e) => onChange(+e.target.value)}
        className="w-full accent-accent"
      />
    </div>
  );
}
