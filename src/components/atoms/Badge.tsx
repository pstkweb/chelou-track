import cn from "../../lib/cn";

type BadgeProps = {
    className?: string;
} & React.HTMLAttributes<HTMLSpanElement>;

export default function Badge({ className = '', children, ...rest }: BadgeProps) {
    const cls = cn([
        'text-xs font-bold uppercase tracking-wider py-1 px-2 rounded bg-chip text-fg2',
        className,
    ]);

    return <span className={cls} {...rest}>{children}</span>;
}