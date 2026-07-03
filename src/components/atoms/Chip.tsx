import cn from '@/lib/cn';

type ChipProps = {
  as?: React.ElementType;
  className?: string;
  children: React.ReactNode;
} & React.HTMLAttributes<HTMLElement>;

export default function Chip({ as: Tag = 'span', className = '', children, ...rest }: ChipProps) {
  const cls = cn(['chip', className]);

  return (
    <Tag className={cls} {...rest}>
      {children}
    </Tag>
  );
}
