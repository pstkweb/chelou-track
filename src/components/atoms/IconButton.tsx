type IconButtonProps = {
  className?: string;
  children: React.ReactNode;
} & React.HTMLAttributes<HTMLButtonElement>;

export default function IconButton({ className = "", children, ...rest }: IconButtonProps) {
  return (
    <button className={["icon-btn", className].filter(Boolean).join(" ")} {...rest}>
      {children}
    </button>
  );
}
