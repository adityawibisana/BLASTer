import * as Dialog from "@radix-ui/react-dialog";
import clsx from "clsx";
import type { ReactNode } from "react";
import { FLAG_LABEL, flagTone } from "../lib/format";
import type { Flag } from "../types";

export function Badge({ tone = "info", children, title }: { tone?: "ok" | "warn" | "bad" | "info"; children: ReactNode; title?: string }) {
  return (
    <span
      title={title}
      className={clsx(
        "inline-flex items-center rounded px-1.5 py-px text-[11.5px] font-medium whitespace-nowrap",
        tone === "ok" && "bg-ok-soft text-ok",
        tone === "warn" && "bg-warn-soft text-warn",
        tone === "bad" && "bg-bad-soft text-bad",
        tone === "info" && "bg-info-soft text-muted",
      )}
    >
      {children}
    </span>
  );
}

/** Status badges. In table cells they stay on one line; hover shows the full list. */
export function Flags({ flags, empty = "OK", wrap = false }: { flags: Flag[]; empty?: string; wrap?: boolean }) {
  if (flags.length === 0) return <Badge tone="ok">{empty}</Badge>;
  return (
    <span className={clsx("inline-flex gap-1", wrap ? "flex-wrap" : "flex-nowrap")} title={flags.map((f) => FLAG_LABEL[f]).join(", ")}>
      {flags.map((f) => (
        <Badge key={f} tone={flagTone(f)}>
          {FLAG_LABEL[f]}
        </Badge>
      ))}
    </span>
  );
}

export function PageHeader({ title, subtitle, actions }: { title: string; subtitle?: ReactNode; actions?: ReactNode }) {
  return (
    <div className="flex items-end justify-between gap-4 px-6 pt-5 pb-4">
      <div className="min-w-0">
        <h1 className="m-0 text-[19px] font-semibold tracking-tight">{title}</h1>
        {subtitle && <div className="mt-1 text-muted">{subtitle}</div>}
      </div>
      {actions && <div className="flex shrink-0 items-center gap-2">{actions}</div>}
    </div>
  );
}

export function Stat({ label, value, tone }: { label: string; value: ReactNode; tone?: "ok" | "warn" | "bad" }) {
  return (
    <div className="min-w-[92px]">
      <div className="label">{label}</div>
      <div
        className={clsx(
          "tabular text-[15px] font-semibold",
          tone === "ok" && "text-ok",
          tone === "warn" && "text-warn",
          tone === "bad" && "text-bad",
        )}
      >
        {value}
      </div>
    </div>
  );
}

export function Empty({ children }: { children: ReactNode }) {
  return <div className="flex h-full items-center justify-center p-10 text-center text-muted">{children}</div>;
}

export function Modal({
  open,
  onOpenChange,
  title,
  children,
  footer,
}: {
  open: boolean;
  onOpenChange: (v: boolean) => void;
  title: string;
  children: ReactNode;
  footer?: ReactNode;
}) {
  return (
    <Dialog.Root open={open} onOpenChange={onOpenChange}>
      <Dialog.Portal>
        <Dialog.Overlay className="fixed inset-0 bg-black/30" />
        <Dialog.Content className="card fixed top-1/2 left-1/2 w-[460px] max-w-[92vw] -translate-x-1/2 -translate-y-1/2 p-5 shadow-xl">
          <Dialog.Title className="m-0 mb-3 text-[16px] font-semibold">{title}</Dialog.Title>
          <Dialog.Description asChild>
            <div>{children}</div>
          </Dialog.Description>
          {footer && <div className="mt-5 flex justify-end gap-2">{footer}</div>}
        </Dialog.Content>
      </Dialog.Portal>
    </Dialog.Root>
  );
}

export function Field({ label, hint, children }: { label: string; hint?: ReactNode; children: ReactNode }) {
  return (
    <label className="flex flex-col gap-1">
      <span className="font-medium">{label}</span>
      {children}
      {hint && <span className="label">{hint}</span>}
    </label>
  );
}
