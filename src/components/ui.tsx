import { useEffect, useRef, useState } from "react";
import { MoreHorizontal, X } from "lucide-react";

/** Shared button styles so every screen uses the same three tiers. */
export const btn = {
  primary: "inline-flex items-center gap-1.5 rounded-md bg-accent px-4 py-1.5 text-sm font-semibold text-ink-950 transition-colors hover:bg-accent/90 disabled:cursor-not-allowed disabled:opacity-50",
  secondary: "inline-flex items-center gap-1.5 rounded-md border border-ink-700 bg-ink-800 px-2.5 py-1 text-xs text-slate-300 transition-colors hover:text-white disabled:opacity-50",
  danger: "inline-flex items-center gap-1.5 rounded-md bg-red-600 px-4 py-1.5 text-sm font-semibold text-white transition-colors hover:bg-red-500",
};

/** Non-blocking replacement for alert(): shows a dismissible toast in the corner. */
export const notify = (msg: unknown) => window.dispatchEvent(new CustomEvent("syntari-toast", { detail: String(msg) }));

export function Toasts() {
  const [items, setItems] = useState<{ id: number; msg: string }[]>([]);
  useEffect(() => {
    const on = (e: Event) => {
      const id = Date.now() + Math.random();
      setItems((l) => [...l, { id, msg: (e as CustomEvent<string>).detail }]);
      setTimeout(() => setItems((l) => l.filter((t) => t.id !== id)), 8000);
    };
    window.addEventListener("syntari-toast", on);
    return () => window.removeEventListener("syntari-toast", on);
  }, []);
  return (
    <div role="status" aria-live="polite" className="pointer-events-none fixed bottom-4 right-4 z-50 flex max-w-sm flex-col gap-2">
      {items.map((t) => (
        <div key={t.id} className="pointer-events-auto flex items-start gap-2 rounded-lg border border-ink-700 bg-ink-800 p-3 text-sm text-slate-200 shadow-xl">
          <span className="flex-1 whitespace-pre-wrap break-words">{t.msg}</span>
          <button aria-label="Dismiss" onClick={() => setItems((l) => l.filter((x) => x.id !== t.id))} className="text-slate-400 hover:text-white"><X size={14} /></button>
        </div>
      ))}
    </div>
  );
}

/** Small "⋯" dropdown: closes on item click, outside click or Escape. */
export function Menu({ label, items, disabled }: { label: string; disabled?: boolean; items: { label: string; onClick: () => void; danger?: boolean; title?: string; disabled?: boolean }[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (e: MouseEvent | KeyboardEvent) => {
      if (e instanceof KeyboardEvent ? e.key === "Escape" : !ref.current?.contains(e.target as Node)) setOpen(false);
    };
    document.addEventListener("mousedown", close);
    document.addEventListener("keydown", close);
    return () => { document.removeEventListener("mousedown", close); document.removeEventListener("keydown", close); };
  }, [open]);
  return (
    <div ref={ref} className="relative">
      <button aria-label={label} title={label} aria-haspopup="menu" aria-expanded={open} disabled={disabled} onClick={() => setOpen((o) => !o)} className={`${btn.secondary} h-7 w-8 justify-center px-0`}>
        <MoreHorizontal size={16} />
      </button>
      {open && (
        <div role="menu" className="absolute right-0 top-full z-40 mt-1 min-w-[12rem] rounded-lg border border-ink-700 bg-ink-800 py-1 shadow-xl">
          {items.map((it) => (
            <button
              key={it.label}
              role="menuitem"
              title={it.title}
              disabled={it.disabled}
              onClick={() => { setOpen(false); it.onClick(); }}
              className={`block w-full px-3 py-1.5 text-left text-sm hover:bg-ink-700 disabled:opacity-50 ${it.danger ? "text-rose-400" : "text-slate-200"}`}
            >{it.label}</button>
          ))}
        </div>
      )}
    </div>
  );
}
