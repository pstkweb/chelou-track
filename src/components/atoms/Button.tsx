import cn from "../../lib/cn";

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> & {
    variant?: 'primary' | 'ghost';
    size?: 'lg';
};

export default function Button({ variant, size, className = '', children, ...rest }: ButtonProps) {
    const cls = cn([
        'inline-flex items-center justify-center gap-2 content-center h-9 py-0 px-4 border border-border2 rounded bg-chip text-fg font-semibold whitespace-nowrap hover:border-fg3 transition',
        variant === 'primary' && 'bg-accent text-accentink border-transparent shadow-[0_6px_18px_-6px_var(--accent)] hover:brightness-[1.06] hover:border-transparent',
        variant === 'ghost' && 'bg-transparent border-transparent hover:bg-chip hover:border-transparent',
        size === 'lg' && 'h-12 py-0 px-5 text-lg',
        className,
    ]);

    return <button className={cls} {...rest}>{children}</button>;
}