import { useEffect, useId, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { Check, ChevronDown } from "lucide-react";
import { useAnchoredPanel } from "../hooks/useAnchoredPanel";

export type SelectOption<T extends string = string> = {
  value: T;
  label: string;
  disabled?: boolean;
};

/**
 * 通用下拉选择器，替代各页裸露的原生 `<select>`。触发按钮沿用输入框外观，
 * 选项面板 portal 到 body 并锚定在触发按钮下方（与上游模型选择器同款定位），
 * 键盘可导航、点外部关闭。打开后焦点落在 listbox，本节用 aria-activedescendant
 * 指示活动项，活动态由键盘与指针共用。
 */
export function Select<T extends string>({
  value,
  options,
  onChange,
  label,
  disabled,
  className,
  placeholder = "请选择",
}: {
  value: T;
  options: readonly SelectOption<T>[];
  onChange: (value: T) => void;
  label: string;
  disabled?: boolean;
  className?: string;
  placeholder?: string;
}) {
  const [open, setOpen] = useState(false);
  const [activeIndex, setActiveIndex] = useState(0);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const listboxId = useId();
  const position = useAnchoredPanel({
    open,
    triggerRef,
    panelRef,
    deps: [options.length],
  });

  const selectedIndex = options.findIndex((option) => option.value === value);
  const selected = selectedIndex >= 0 ? options[selectedIndex] : null;

  const firstEnabled = () => {
    const index = options.findIndex((option) => !option.disabled);
    return index < 0 ? 0 : index;
  };
  const lastEnabled = () => {
    for (let index = options.length - 1; index >= 0; index -= 1) {
      if (!options[index].disabled) return index;
    }
    return 0;
  };

  const openPanel = () => {
    setActiveIndex(selectedIndex >= 0 ? selectedIndex : firstEnabled());
    setOpen(true);
  };

  const closePanel = (refocus = true) => {
    setOpen(false);
    if (refocus) triggerRef.current?.focus();
  };

  const commit = (index: number) => {
    const option = options[index];
    if (!option || option.disabled) return;
    if (option.value !== value) onChange(option.value);
    closePanel();
  };

  const moveActive = (delta: number) => {
    setActiveIndex((current) => {
      let next = current;
      for (let step = 0; step < options.length; step += 1) {
        next = (next + delta + options.length) % options.length;
        if (!options[next].disabled) return next;
      }
      return current;
    });
  };

  useEffect(() => {
    if (!open) return;
    panelRef.current?.focus();

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (panelRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  useEffect(() => {
    if (!open || !position) return;
    panelRef.current
      ?.querySelector<HTMLElement>(".select-option.is-active")
      ?.scrollIntoView({ block: "nearest" });
  }, [open, position, activeIndex]);

  const onTriggerKeyDown = (event: KeyboardEvent<HTMLButtonElement>) => {
    if (open) return;
    if (
      event.key === "ArrowDown" ||
      event.key === "ArrowUp" ||
      event.key === "Enter" ||
      event.key === " "
    ) {
      event.preventDefault();
      openPanel();
    }
  };

  const onPanelKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    switch (event.key) {
      case "ArrowDown":
        event.preventDefault();
        moveActive(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveActive(-1);
        break;
      case "Home":
        event.preventDefault();
        setActiveIndex(firstEnabled());
        break;
      case "End":
        event.preventDefault();
        setActiveIndex(lastEnabled());
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        commit(activeIndex);
        break;
      case "Escape":
        event.preventDefault();
        event.stopPropagation();
        closePanel();
        break;
      case "Tab":
        setOpen(false);
        break;
      default:
        break;
    }
  };

  return (
    <>
      <button
        type="button"
        className={className ? `select-trigger ${className}` : "select-trigger"}
        ref={triggerRef}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listboxId : undefined}
        aria-label={label}
        disabled={disabled}
        onClick={() => (open ? closePanel(false) : openPanel())}
        onKeyDown={onTriggerKeyDown}
      >
        <span className="select-value">
          {selected ? selected.label : <span className="select-placeholder">{placeholder}</span>}
        </span>
        <ChevronDown className="select-chevron" aria-hidden="true" />
      </button>

      {open
        ? createPortal(
            <div
              className="select-panel"
              id={listboxId}
              role="listbox"
              aria-label={label}
              tabIndex={-1}
              ref={panelRef}
              data-escape-scope="local"
              aria-activedescendant={
                options.length > 0 ? `${listboxId}-option-${activeIndex}` : undefined
              }
              style={{
                top: position?.top ?? -9999,
                left: position?.left ?? -9999,
                width: position?.width,
                visibility: position ? "visible" : "hidden",
              }}
              onKeyDown={onPanelKeyDown}
            >
              {options.length === 0 ? (
                <p className="select-empty">没有可选项。</p>
              ) : (
                options.map((option, index) => {
                  const isSelected = option.value === value;
                  const classes = [
                    "select-option",
                    index === activeIndex ? "is-active" : "",
                    isSelected ? "is-selected" : "",
                    option.disabled ? "is-disabled" : "",
                  ]
                    .filter(Boolean)
                    .join(" ");
                  return (
                    <div
                      key={index}
                      id={`${listboxId}-option-${index}`}
                      role="option"
                      aria-selected={isSelected}
                      aria-disabled={option.disabled || undefined}
                      className={classes}
                      onPointerMove={() => {
                        if (!option.disabled) setActiveIndex(index);
                      }}
                      onPointerDown={(event) => event.preventDefault()}
                      onClick={() => commit(index)}
                    >
                      <span className="select-option-label">{option.label}</span>
                      {isSelected ? <Check className="select-check" aria-hidden="true" /> : null}
                    </div>
                  );
                })
              )}
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
