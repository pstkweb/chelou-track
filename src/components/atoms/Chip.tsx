import cn from "../../lib/cn";

type ChipProps = {
    as?: React.ElementType;
    className?: string;
    children: React.ReactNode;
} & React.HTMLAttributes<HTMLElement>;

export default function Chip({ as: Tag = 'span', className = '', children, ...rest }: ChipProps) {
    const cls = cn([
        'inline-flex items-center gap-1.5 h-6 py-0 px-2.5 rounded-full bg-chip text-fg2 text-xs font-semibold border border-border',
        className,
    ]);

    return <Tag
        className={cls}
        {...rest}
    >{children}</Tag>;
}