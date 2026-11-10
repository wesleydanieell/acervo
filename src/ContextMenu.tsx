import { useEffect, useRef, type ReactNode } from "react";

export type ContextAction = {
  label: string;
  icon: ReactNode;
  onSelect: () => void;
  disabled?: boolean;
};

type Props = {
  x: number;
  y: number;
  title: string;
  actions: ContextAction[];
  onClose: () => void;
};

export function ContextMenu({ x, y, title, actions, onClose }: Props) {
  const menuRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    const outside = (event: PointerEvent) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    const scroll = (event: Event) => {
      if (!menuRef.current?.contains(event.target as Node)) onClose();
    };
    window.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", escape);
    window.addEventListener("scroll", scroll, true);
    menuRef.current?.querySelector<HTMLButtonElement>("button:not(:disabled)")?.focus();
    return () => {
      window.removeEventListener("pointerdown", outside);
      window.removeEventListener("keydown", escape);
      window.removeEventListener("scroll", scroll, true);
    };
  }, [onClose]);

  const width = 246;
  const height = Math.min(440, 52 + actions.length * 43);
  const left = Math.max(10, Math.min(x, window.innerWidth - width - 10));
  const top = Math.max(10, Math.min(y, window.innerHeight - height - 10));

  return <div ref={menuRef} className="acervo-context-menu glass-panel" role="menu"
    aria-label={title} style={{ left, top, width }}
    onKeyDown={(event) => {
      if (event.key !== "ArrowDown" && event.key !== "ArrowUp") return;
      const buttons = [...(menuRef.current?.querySelectorAll<HTMLButtonElement>("button:not(:disabled)") || [])];
      if (!buttons.length) return;
      event.preventDefault();
      const active = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const delta = event.key === "ArrowDown" ? 1 : -1;
      buttons[(active + delta + buttons.length) % buttons.length]?.focus();
    }}>
    <div className="acervo-context-title" title={title}>{title}</div>
    {actions.map((action) => <button type="button" key={action.label} role="menuitem"
      disabled={action.disabled} onClick={() => { onClose(); action.onSelect(); }}>
      {action.icon}<span>{action.label}</span>
    </button>)}
  </div>;
}
