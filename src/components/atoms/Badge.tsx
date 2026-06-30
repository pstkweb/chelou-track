import cn from "@/lib/cn";

type BadgeProps = {
  className?: string;
} & React.HTMLAttributes<HTMLSpanElement>;

export default function Badge({ className = "", children, ...rest }: BadgeProps) {
  const cls = cn(["badge-lvl", className]);

  return (
    <span className={cls} {...rest}>
      {children}
    </span>
  );
}
