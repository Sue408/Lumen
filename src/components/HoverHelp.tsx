import {
  useEffect,
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";

/**
 * 通用悬停说明：一个图标按钮，悬停 / 聚焦时在旁侧弹出浮层。
 * 浮层 portal 到 body 并用 fixed 定位，避免被容器的 overflow 裁切
 * （与品牌图标选择器同一套定位思路）。
 */
export function HoverHelp({
  label,
  icon,
  panel,
  triggerClassName,
  panelClassName,
  panelId,
}: {
  label: string;
  icon: ReactNode;
  panel: ReactNode;
  triggerClassName: string;
  panelClassName: string;
  panelId?: string;
}) {
  const generatedId = useId();
  const id = panelId ?? generatedId;
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);

  useLayoutEffect(() => {
    if (!open) return;
    const trigger = triggerRef.current;
    const panelEl = panelRef.current;
    if (!trigger || !panelEl) return;

    const place = () => {
      const margin = 8;
      const rect = trigger.getBoundingClientRect();
      const panelRect = panelEl.getBoundingClientRect();
      let top = rect.bottom + margin;
      if (top + panelRect.height > window.innerHeight - margin) {
        top = Math.max(margin, rect.top - margin - panelRect.height);
      }
      let left = rect.right - panelRect.width;
      if (left < margin) left = margin;
      if (left + panelRect.width > window.innerWidth - margin) {
        left = Math.max(margin, window.innerWidth - margin - panelRect.width);
      }
      setPosition({ top, left });
    };

    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open]);

  return (
    <>
      <button
        className={triggerClassName}
        type="button"
        ref={triggerRef}
        aria-label={label}
        aria-describedby={open ? id : undefined}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
      >
        {icon}
      </button>

      {open
        ? createPortal(
            <div
              className={panelClassName}
              id={id}
              role="tooltip"
              ref={panelRef}
              style={{
                top: position?.top ?? -9999,
                left: position?.left ?? -9999,
                visibility: position ? "visible" : "hidden",
              }}
            >
              {panel}
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
