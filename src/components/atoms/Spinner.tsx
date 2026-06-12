import cn from "../../lib/cn";

type SpinnerProps = {
    light?: boolean;
};

export default function Spinner({ light }: SpinnerProps) {
    const cls = cn([
        'size-4 rounded-full border border-2 border-border2 border-t-accent animate-spin',
        light && 'border-[rgba(0,0,0,.25)] border-t-accentink',
    ]);

    return <div className={cls} />;
}