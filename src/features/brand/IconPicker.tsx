import {
  useEffect,
  useLayoutEffect,
  useRef,
  useState,
  type KeyboardEvent,
  type ReactNode,
} from "react";
import { createPortal } from "react-dom";
import { BrandGlyph } from "./BrandMark";
import { BRAND_IDS, BRAND_LABELS, isBrandId, type BrandId } from "./brand";
import type { IconTint } from "../../services/config";

const COLUMNS = 6;
const NAV_KEYS = ["ArrowLeft", "ArrowRight", "ArrowUp", "ArrowDown", "Home", "End"];
const TINTS: IconTint[] = ["ink", "brand"];

type IconPickerProps = {
  icon: string | null;
  auto: BrandId | null;
  tint: IconTint;
  size?: number;
  fallback: ReactNode;
  glyphClassName?: string;
  disabled?: boolean;
  enabled?: boolean;
  onChange: (icon: BrandId | null) => void;
  onTintChange: (tint: IconTint) => void;
};

/**
 * Anchored brand-icon picker. The trigger is a button so the header glyph stays
 * clickable; the panel is a paper card positioned next to it (never a centered
 * modal, per the config-page grammar).
 */
export function IconPicker({
  icon,
  auto,
  tint,
  size = 22,
  fallback,
  glyphClassName,
  disabled,
  enabled = true,
  onChange,
  onTintChange,
}: IconPickerProps) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);
  const active = isBrandId(icon) ? icon : auto;
  const previewTint: IconTint = enabled ? tint : "ink";

  const close = () => setOpen(false);

  useLayoutEffect(() => {
    if (!open) return;
    const trigger = triggerRef.current;
    const panel = panelRef.current;
    if (!trigger || !panel) return;

    const place = () => {
      const margin = 8;
      const rect = trigger.getBoundingClientRect();
      const panelRect = panel.getBoundingClientRect();
      let top = rect.bottom + margin;
      if (top + panelRect.height > window.innerHeight - margin) {
        top = Math.max(margin, rect.top - margin - panelRect.height);
      }
      let left = rect.left;
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
    const panel = panelRef.current;
    const focusTarget =
      panel?.querySelector<HTMLButtonElement>(".is-selected") ??
      panel?.querySelector<HTMLButtonElement>("button");
    focusTarget?.focus();

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (panelRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      close();
    };
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key !== "Escape") return;
      event.stopPropagation();
      triggerRef.current?.focus();
      close();
    };

    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown, true);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown, true);
    };
  }, [open]);

  const select = (value: BrandId | null) => {
    onChange(value);
    triggerRef.current?.focus();
    close();
  };

  const handlePanelKeys = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!NAV_KEYS.includes(event.key)) return;
    const cells = Array.from(
      panelRef.current?.querySelectorAll<HTMLButtonElement>("button.brand-picker-cell") ?? [],
    );
    if (cells.length === 0) return;
    const focused = document.activeElement as HTMLElement | null;
    const index = focused ? cells.indexOf(focused as HTMLButtonElement) : -1;
    if (index < 0) return;
    event.preventDefault();
    let next = index;
    if (event.key === "ArrowLeft") next = Math.max(0, index - 1);
    else if (event.key === "ArrowRight") next = Math.min(cells.length - 1, index + 1);
    else if (event.key === "ArrowUp") next = Math.max(0, index - COLUMNS);
    else if (event.key === "ArrowDown") next = Math.min(cells.length - 1, index + COLUMNS);
    else if (event.key === "Home") next = 0;
    else if (event.key === "End") next = cells.length - 1;
    cells[next]?.focus();
  };

  return (
    <>
      <button
        className="brand-picker-trigger"
        type="button"
        ref={triggerRef}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label="选择图标"
        title="选择图标"
        disabled={disabled}
        onClick={() => setOpen((value) => !value)}
      >
        <BrandGlyph brand={active} size={size} fallback={fallback} className={glyphClassName} tint={previewTint} />
      </button>

      {open
        ? createPortal(
            <div
              className="brand-picker"
              role="dialog"
              aria-label="选择图标"
              ref={panelRef}
              onKeyDown={handlePanelKeys}
              style={{
                top: position?.top ?? -9999,
                left: position?.left ?? -9999,
                visibility: position ? "visible" : "hidden",
              }}
            >
              <button
                className={`brand-picker-auto${icon === null ? " is-selected" : ""}`}
                type="button"
                aria-pressed={icon === null}
                onClick={() => select(null)}
              >
                <BrandGlyph brand={auto} size={18} fallback={fallback} tint={previewTint} />
                <span>自动识别{auto ? ` · ${BRAND_LABELS[auto]}` : "（无匹配）"}</span>
              </button>
              <div className="brand-picker-tint" role="group" aria-label="图标着色">
                {TINTS.map((value) => (
                  <button
                    className={tint === value ? "is-selected" : ""}
                    type="button"
                    key={value}
                    aria-pressed={tint === value}
                    onClick={() => onTintChange(value)}
                  >
                    {value === "ink" ? "墨色" : "品牌色"}
                  </button>
                ))}
              </div>
              <div className="brand-picker-grid">
                {BRAND_IDS.map((brand) => (
                  <button
                    className={`brand-picker-cell${icon === brand ? " is-selected" : ""}`}
                    type="button"
                    key={brand}
                    title={BRAND_LABELS[brand]}
                    aria-label={BRAND_LABELS[brand]}
                    aria-pressed={icon === brand}
                    onClick={() => select(brand)}
                  >
                    <BrandGlyph brand={brand} size={20} fallback={null} tint={previewTint} />
                  </button>
                ))}
              </div>
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
