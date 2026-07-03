import { Search } from 'lucide-react';
import type { ChangeEventHandler } from 'react';
import Field from '@/components/atoms/Field';

type SearchFieldProps = {
  onChange: ChangeEventHandler<HTMLInputElement, HTMLInputElement>;
  placeholder: string;
  value: string;
};

export default function SearchField({
  value,
  onChange,
  placeholder = 'Rechercher…',
}: SearchFieldProps) {
  return (
    <div className="relative w-3xs">
      <Search size={17} className="absolute top-2.5 left-2.5 text-fg3" />
      <Field placeholder={placeholder} value={value} onChange={onChange} />
    </div>
  );
}
