import cn from '@/lib/cn';

type FieldProps = React.InputHTMLAttributes<HTMLInputElement> & {
  className?: string;
};

export default function Field({ className = '', ...rest }: FieldProps) {
  const cls = cn(['search', className]);

  return <input className={cls} {...rest} />;
}
