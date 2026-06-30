import { ChevronRight } from "lucide-react";
import React from "react";
import type { BreadcrumbItem } from "@/contexts/BreadcrumbContext";
import cn from "@/lib/cn";

type BreadcrumbProps = {
  items: BreadcrumbItem[];
};

export default function Breadcrumb({ items }: BreadcrumbProps) {
  return (
    <div className="ml-2 flex items-center gap-1.5">
      {items.map((c, i) => {
        const last = i === items.length - 1;
        const hasNoLink = c.onClick === undefined;

        return (
          <React.Fragment key={c.label}>
            <ChevronRight size={13} className="text-fg3 opacity-60" />
            <button
              type="button"
              className={cn(
                "max-w-56 cursor-pointer overflow-hidden text-ellipsis whitespace-nowrap border-none bg-none p-0 font-medium text-fg3 text-xs",
                last && "font-semibold text-fg",
                hasNoLink && "cursor-default!",
              )}
              onClick={c.onClick}
              disabled={hasNoLink}
            >
              {c.label}
            </button>
          </React.Fragment>
        );
      })}
    </div>
  );
}
