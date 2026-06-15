import cn from "../../lib/cn";

type SpinnerProps = {
  light?: boolean;
  className?: string;
};

export default function Spinner({ light, className }: SpinnerProps) {
  const cls = cn([
    "size-4 rounded-full border border-2 border-border2 border-t-accent animate-spin",
    light && "border-[rgba(0,0,0,.25)] border-t-accentink",
    className,
  ]);

  return <div className={cls} />;
}
