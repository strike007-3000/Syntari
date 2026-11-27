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

/** Small "⋯" dropdown following the WAI-ARIA menu button pattern: focus moves into the menu on open,
 *  arrow keys / Home / End move between items, Escape or Tab closes it, and focus returns to the trigger. */
export function Menu({ label, items, disabled }: { label: string; disabled?: boolean; items: { label: string; onClick: () => void; danger?: boolean; title?: string; disabled?: boolean }[] }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menu = useRef<HTMLDivElement>(null);
  const enabled = () => [...(menu.current?.querySelectorAll<HTMLButtonElement>('[role="menuitem"]:not(:disabled)') ?? [])];
  const close = (refocus: boolean) => { setOpen(false); if (refocus) trigger.current?.focus(); };

  useEffect(() => {
    if (!open) return;
    enabled()[0]?.focus();
    const outside = (e: MouseEvent) => { if (!ref.current?.contains(e.target as Node)) setOpen(false); };
    document.addEventListener("mousedown", outside);
    return () => document.removeEventListener("mousedown", outside);
  }, [open]);

  const onKeyDown = (e: React.KeyboardEvent) => {
    const list = enabled();
    const i = list.indexOf(document.activeElement as HTMLButtonElement);
    const move = (n: number) => { e.preventDefault(); list[(n + list.length) % list.length]?.focus(); };
    if (e.key === "ArrowDown") move(i + 1);
    else if (e.key === "ArrowUp") move(i - 1);
    else if (e.key === "Home") move(0);
    else if (e.key === "End") move(list.length - 1);
    else if (e.key === "Escape") { e.preventDefault(); close(true); }
    else if (e.key === "Tab") close(false); // let Tab carry on to the next control
  };

  return (
    <div ref={ref} className="relative">
      <button
        ref={trigger}
        aria-label={label}
        title={label}
        aria-haspopup="menu"
        aria-expanded={open}
        disabled={disabled}
        onClick={() => setOpen((o) => !o)}
        onKeyDown={(e) => { if (e.key === "ArrowDown" || e.key === "ArrowUp") { e.preventDefault(); setOpen(true); } }}
        className={`${btn.secondary} h-7 w-8 justify-center px-0`}
      >
        <MoreHorizontal size={16} />
      </button>
      {open && (
        <div ref={menu} role="menu" aria-label={label} onKeyDown={onKeyDown} className="absolute right-0 top-full z-40 mt-1 min-w-[12rem] rounded-lg border border-ink-700 bg-ink-800 py-1 shadow-xl">
          {items.map((it) => (
            <button
              key={it.label}
              role="menuitem"
              tabIndex={-1}
              title={it.title}
              disabled={it.disabled}
              onClick={() => { close(true); it.onClick(); }}
              className={`block w-full px-3 py-1.5 text-left text-sm hover:bg-ink-700 focus:bg-ink-700 disabled:opacity-50 ${it.danger ? "text-rose-400" : "text-slate-200"}`}
            >{it.label}</button>
          ))}
        </div>
      )}
    </div>
  );
}
