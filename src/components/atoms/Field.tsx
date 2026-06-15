import cn from "../../lib/cn";

type FieldProps = React.InputHTMLAttributes<HTMLInputElement> & {
  className?: string;
};

export default function Field({ className = "", ...rest }: FieldProps) {
  const cls = cn([
    "h-9 w-full rounded border border-border bg-bg text-fg [font:inherit] p-0 pl-3 pr-9 outline-none focus:border-accent",
    className,
  ]);

  return <input className={cls} {...rest} />;
}
