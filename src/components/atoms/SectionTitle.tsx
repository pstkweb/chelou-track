import type { LucideIcon } from 'lucide-react';

/** Titre de section du tiroir des réglages audio. */
export default function SectionTitle({
  icon: I,
  children,
}: {
  icon: LucideIcon;
  children: React.ReactNode;
}) {
  return (
    <h3 className="mb-3 flex items-center gap-2 font-bold text-[13px] text-fg2 uppercase tracking-[0.04em] [&_svg]:text-fg3">
      <I size={15} /> {children}
    </h3>
  );
}
