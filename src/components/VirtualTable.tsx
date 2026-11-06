import { useMemo, useRef, useState, type ReactNode } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import clsx from "clsx";
import { ArrowDown, ArrowUp } from "lucide-react";

export interface Column<T> {
  key: string;
  header: ReactNode;
  width: number;
  align?: "left" | "right";
  render: (row: T) => ReactNode;
  /** Sort value; columns without one are not sortable. */
  sort?: (row: T) => string | number | null;
  title?: string;
}

/** Table that only renders visible rows, so 1000+ rows stay smooth. */
export function VirtualTable<T>({
  rows,
  columns,
  rowKey,
  onRowClick,
  selectedKey,
  rowHeight = 34,
  defaultSort,
}: {
  rows: T[];
  columns: Column<T>[];
  rowKey: (row: T) => string;
  onRowClick?: (row: T) => void;
  selectedKey?: string | null;
  rowHeight?: number;
  defaultSort?: { key: string; desc: boolean };
}) {
  const [sort, setSort] = useState<{ key: string; desc: boolean } | null>(defaultSort ?? null);
  const sorted = useMemo(() => {
    const col = sort && columns.find((c) => c.key === sort.key);
    if (!col?.sort) return rows;
    const get = col.sort;
    return [...rows].sort((a, b) => {
      const va = get(a);
      const vb = get(b);
      if (va === vb) return 0;
      if (va === null) return 1;
      if (vb === null) return -1;
      const cmp = typeof va === "number" && typeof vb === "number" ? va - vb : String(va).localeCompare(String(vb), undefined, { numeric: true });
      return sort!.desc ? -cmp : cmp;
    });
  }, [rows, columns, sort]);

  const parent = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: sorted.length,
    getScrollElement: () => parent.current,
    estimateSize: () => rowHeight,
    overscan: 12,
  });
  const totalWidth = columns.reduce((a, c) => a + c.width, 0);
  const template = columns.map((c) => `${c.width}px`).join(" ");

  return (
    <div ref={parent} className="card relative h-full overflow-auto">
      <div style={{ minWidth: totalWidth }}>
        <div
          className="sticky top-0 z-10 grid border-b border-line bg-panel-2 text-[12px] font-semibold text-muted"
          style={{ gridTemplateColumns: template }}
        >
          {columns.map((c) => (
            <button
              key={c.key}
              disabled={!c.sort}
              title={c.title}
              onClick={() => setSort((s) => (s?.key === c.key ? { key: c.key, desc: !s.desc } : { key: c.key, desc: false }))}
              className={clsx(
                "flex h-8 items-center gap-1 px-2.5 disabled:cursor-default",
                c.align === "right" ? "justify-end text-right" : "text-left",
                c.sort && "hover:text-ink",
              )}
            >
              {c.header}
              {sort?.key === c.key && (sort.desc ? <ArrowDown size={12} /> : <ArrowUp size={12} />)}
            </button>
          ))}
        </div>
        <div style={{ height: virtualizer.getTotalSize(), position: "relative" }}>
          {virtualizer.getVirtualItems().map((v) => {
            const row = sorted[v.index];
            const key = rowKey(row);
            return (
              <div
                key={key}
                onClick={() => onRowClick?.(row)}
                className={clsx(
                  "absolute left-0 grid w-full items-center border-b border-line/70",
                  onRowClick && "cursor-pointer hover:bg-panel-2",
                  selectedKey === key && "bg-accent-soft hover:bg-accent-soft",
                )}
                style={{ gridTemplateColumns: template, height: v.size, transform: `translateY(${v.start}px)` }}
              >
                {columns.map((c) => (
                  <div
                    key={c.key}
                    className={clsx("tabular truncate px-2.5", c.align === "right" && "text-right")}
                  >
                    {c.render(row)}
                  </div>
                ))}
              </div>
            );
          })}
        </div>
        {sorted.length === 0 && <div className="p-6 text-center text-muted">Nothing to show.</div>}
      </div>
    </div>
  );
}
