import { useEffect, useRef, type KeyboardEvent, type ReactNode } from "react";

type RegisterListProps = {
  ariaLabel: string;
  selectedKey?: string | null;
  children: ReactNode;
};

const NAV_KEYS = ["ArrowUp", "ArrowDown", "Home", "End"];

/**
 * Object index for the configuration registers. Adds roving keyboard focus on
 * top of the plain list of selectable buttons, and keeps the selected item in
 * view when selection changes from elsewhere.
 */
export function RegisterList({ ariaLabel, selectedKey, children }: RegisterListProps) {
  const listRef = useRef<HTMLElement>(null);

  useEffect(() => {
    const list = listRef.current;
    if (!list || !selectedKey) return;
    const item = list.querySelector<HTMLElement>(
      `[data-register-key="${CSS.escape(selectedKey)}"]`,
    );
    if (!item) return;
    if (list.contains(document.activeElement)) return;
    const listRect = list.getBoundingClientRect();
    const itemRect = item.getBoundingClientRect();
    if (itemRect.top < listRect.top) {
      list.scrollTop -= listRect.top - itemRect.top;
    } else if (itemRect.bottom > listRect.bottom) {
      list.scrollTop += itemRect.bottom - listRect.bottom;
    }
  }, [selectedKey]);

  const handleKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (!NAV_KEYS.includes(event.key)) return;
    const list = listRef.current;
    if (!list) return;
    const items = Array.from(
      list.querySelectorAll<HTMLButtonElement>("button.register-select"),
    );
    if (items.length === 0) return;

    const focused = document.activeElement as HTMLElement | null;
    const focusedIndex = focused ? items.indexOf(focused as HTMLButtonElement) : -1;
    const selectedIndex = items.findIndex((item) => item.classList.contains("is-selected"));
    const base = focusedIndex >= 0 ? focusedIndex : selectedIndex >= 0 ? selectedIndex : 0;

    let next = base;
    if (event.key === "ArrowUp") next = Math.max(0, base - 1);
    else if (event.key === "ArrowDown") next = Math.min(items.length - 1, base + 1);
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = items.length - 1;

    event.preventDefault();
    items[next]?.focus();
  };

  return (
    <nav className="register" aria-label={ariaLabel} ref={listRef} onKeyDown={handleKeyDown}>
      {children}
    </nav>
  );
}
