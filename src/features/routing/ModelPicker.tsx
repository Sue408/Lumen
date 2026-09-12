import {
  useEffect,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type KeyboardEvent,
} from "react";
import { createPortal } from "react-dom";
import { Plus } from "lucide-react";
import { BrandGlyph } from "../brand/BrandMark";
import { markTint, type ModelMark } from "./routingAppearance";
import type { UpstreamModel } from "../../services/config";

export type ModelGroup = { id: string; name: string; models: UpstreamModel[] };

const NAV_KEYS = ["ArrowDown", "ArrowUp", "Home", "End"];

/**
 * 锚定式上游模型选择面板。触发按钮挂在目标区标题旁，面板 portal 到 body
 * （与品牌图标选择器同款做法），按提供商分组、可搜索、键盘可导航。
 */
export function ModelPicker({
  groups,
  brands,
  taken,
  disabled,
  onSelect,
}: {
  groups: ModelGroup[];
  brands: Map<string, ModelMark>;
  taken: Set<string>;
  disabled?: boolean;
  onSelect: (modelId: string) => void;
}) {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);

  const filtered = useMemo(() => {
    const needle = query.trim().toLowerCase();
    if (!needle) return groups;
    return groups
      .map((group) => ({
        ...group,
        models: group.models.filter((model) =>
          `${group.name} ${model.displayName} ${model.modelId}`.toLowerCase().includes(needle),
        ),
      }))
      .filter((group) => group.models.length > 0);
  }, [groups, query]);

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
    setQuery("");
    panelRef.current?.querySelector<HTMLInputElement>(".model-picker-search")?.focus();

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

  const handlePanelKeys = (event: KeyboardEvent<HTMLDivElement>) => {
    if (!NAV_KEYS.includes(event.key)) return;
    const cells = Array.from(
      panelRef.current?.querySelectorAll<HTMLButtonElement>("button.model-picker-cell:not([disabled])") ?? [],
    );
    if (cells.length === 0) return;
    const focused = document.activeElement as HTMLElement | null;
    const index = focused ? cells.indexOf(focused as HTMLButtonElement) : -1;
    event.preventDefault();
    if (event.key === "Home") {
      cells[0]?.focus();
      return;
    }
    if (event.key === "End") {
      cells[cells.length - 1]?.focus();
      return;
    }
    if (index < 0) {
      cells[0]?.focus();
      return;
    }
    const next = event.key === "ArrowDown" ? index + 1 : index - 1;
    cells[Math.min(Math.max(next, 0), cells.length - 1)]?.focus();
  };

  const pick = (modelId: string) => {
    onSelect(modelId);
    close();
  };

  return (
    <>
      <button
        className="text-action model-picker-trigger"
        type="button"
        ref={triggerRef}
        aria-haspopup="dialog"
        aria-expanded={open}
        disabled={disabled || groups.length === 0}
        onClick={() => setOpen((value) => !value)}
      >
        <Plus aria-hidden="true" />
        添加目标
      </button>

      {open
        ? createPortal(
            <div
              className="model-picker"
              role="dialog"
              aria-label="选择上游模型"
              ref={panelRef}
              onKeyDown={handlePanelKeys}
              style={{
                top: position?.top ?? -9999,
                left: position?.left ?? -9999,
                visibility: position ? "visible" : "hidden",
              }}
            >
              <input
                className="model-picker-search"
                type="search"
                value={query}
                placeholder="搜索模型 / 提供商…"
                spellCheck={false}
                onChange={(event) => setQuery(event.target.value)}
              />
              <div className="model-picker-list">
                {filtered.length === 0 ? (
                  <p className="model-picker-empty">没有匹配的上游模型。</p>
                ) : (
                  filtered.map((group) => (
                    <div className="model-picker-group" key={group.id}>
                      <div className="model-picker-group-name">{group.name}</div>
                      {group.models.map((model) => {
                        const mark = brands.get(model.id);
                        const used = taken.has(model.id);
                        return (
                          <button
                            className="model-picker-cell"
                            type="button"
                            key={model.id}
                            disabled={used}
                            onClick={() => pick(model.id)}
                          >
                            <BrandGlyph
                              brand={mark?.brand ?? null}
                              tint={markTint(mark)}
                              size={18}
                              fallback={null}
                            />
                            <span className="model-picker-name">{model.displayName}</span>
                            <code className="model-picker-id">{model.modelId}</code>
                            {used ? <span className="model-picker-used">已添加</span> : null}
                          </button>
                        );
                      })}
                    </div>
                  ))
                )}
              </div>
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
