import cn from "../../lib/cn";
import { ChevronRight } from "lucide-react";
import React from "react";

type BreadcrumbProps = {
    items: { label: string, onClick?: () => void }[]
}

export default function Breadcrumb({ items }: BreadcrumbProps) {
    return (
        <div className="flex items-center gap-1.5 ml-2">
            {items.map((c, i) => {
                const last = i === items.length - 1;
                const hasNoLink = c.onClick === undefined;

                return (
                    <React.Fragment key={i}>
                        <ChevronRight size={13} className="text-fg3 opacity-60" />
                        <button className={cn('bg-none border-none text-fg3 text-xs font-medium cursor-pointer max-w-56 overflow-hidden text-ellipsis whitespace-nowrap p-0', last && 'text-fg font-semibold', hasNoLink && 'cursor-default!')} onClick={c.onClick} disabled={hasNoLink}>
                            {c.label}
                        </button>
                    </React.Fragment>
                )
            })}
        </div>
    );
}