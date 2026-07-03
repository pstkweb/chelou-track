import cn from '@/lib/cn';

type ButtonProps = React.ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: 'primary' | 'ghost' | undefined;
  size?: 'lg';
};

export default function Button({
  variant = undefined,
  size,
  className = '',
  children,
  ...rest
}: ButtonProps) {
  const cls = cn([
    'btn',
    variant === 'primary' && 'btn-primary',
    variant === 'ghost' && 'btn-ghost',
    size === 'lg' && 'btn-lg',
    className,
  ]);

  return (
    <button className={cls} {...rest}>
      {children}
    </button>
  );
}
