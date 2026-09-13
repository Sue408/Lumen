import { useEffect, useRef, useState, type KeyboardEvent } from "react";
import { createPortal } from "react-dom";
import { CalendarDays, ChevronLeft, ChevronRight } from "lucide-react";
import { useAnchoredPanel } from "../hooks/useAnchoredPanel";
import {
  addDays,
  addMonths,
  formatFullDate,
  formatMonthLabel,
  formatYearLabel,
  fromISODate,
  isSameDay,
  monthGrid,
  startOfDay,
  startOfMonth,
  toISODate,
  weekdayHeaders,
} from "../lib/date";

const MONTHS = Array.from({ length: 12 }, (_, index) => index);

function rangeText(from: string, to: string): string {
  if (from && to) return `${from} – ${to}`;
  if (from) return `从 ${from} 起`;
  if (to) return `至 ${to}`;
  return "";
}

/**
 * 日期范围选择器：一个入口、一个日历，点起点再点终点成区间。
 * 触发按钮沿用字段外观，面板 portal 到 body 并锚定（复用 useAnchoredPanel）。
 * 值仍是 ISO 字符串对，父层维持 from / to 两个状态不变。
 */
export function DateRangePicker({
  from,
  to,
  onChange,
  disabled,
  className,
}: {
  from: string;
  to: string;
  onChange: (from: string, to: string) => void;
  disabled?: boolean;
  className?: string;
}) {
  const [open, setOpen] = useState(false);
  const [view, setView] = useState<"days" | "months">("days");
  const [viewMonth, setViewMonth] = useState(() => startOfMonth(new Date()));
  const [focusedDate, setFocusedDate] = useState(() => startOfDay(new Date()));
  const [pendingAnchor, setPendingAnchor] = useState<Date | null>(null);
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const position = useAnchoredPanel({
    open,
    triggerRef,
    panelRef,
    width: "content",
    deps: [view, viewMonth],
  });

  const today = startOfDay(new Date());
  const text = rangeText(from, to);

  const closePanel = (refocus = true) => {
    setPendingAnchor(null);
    setOpen(false);
    if (refocus) triggerRef.current?.focus();
  };

  const openPanel = () => {
    const seed = fromISODate(from) ?? today;
    setViewMonth(startOfMonth(seed));
    setFocusedDate(seed);
    setView("days");
    setPendingAnchor(null);
    setOpen(true);
  };

  const commit = (date: Date) => {
    if (pendingAnchor) {
      const start = toISODate(pendingAnchor);
      const end = toISODate(date);
      onChange(start <= end ? start : end, start <= end ? end : start);
      closePanel();
      return;
    }
    onChange(toISODate(date), "");
    setPendingAnchor(date);
    setFocusedDate(date);
  };

  const moveFocus = (days: number) => {
    const next = addDays(focusedDate, days);
    if (next.getMonth() !== viewMonth.getMonth() || next.getFullYear() !== viewMonth.getFullYear()) {
      setViewMonth(startOfMonth(next));
    }
    setFocusedDate(next);
  };

  const shiftMonth = (amount: number) => {
    const next = addMonths(viewMonth, amount);
    setViewMonth(next);
    setFocusedDate(next);
  };

  useEffect(() => {
    if (!open) return;
    panelRef.current?.focus();

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (panelRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      setPendingAnchor(null);
      setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  useEffect(() => {
    if (!open || view !== "days") return;
    panelRef.current
      ?.querySelector<HTMLButtonElement>(`[data-date="${toISODate(focusedDate)}"]`)
      ?.focus();
  }, [open, view, focusedDate, viewMonth]);

  const onPanelKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (view === "months") {
      if (event.key === "Escape") {
        event.preventDefault();
        event.stopPropagation();
        closePanel();
      }
      return;
    }
    switch (event.key) {
      case "ArrowLeft":
        event.preventDefault();
        moveFocus(-1);
        break;
      case "ArrowRight":
        event.preventDefault();
        moveFocus(1);
        break;
      case "ArrowUp":
        event.preventDefault();
        moveFocus(-7);
        break;
      case "ArrowDown":
        event.preventDefault();
        moveFocus(7);
        break;
      case "Home":
        event.preventDefault();
        moveFocus(-((focusedDate.getDay() + 6) % 7));
        break;
      case "End":
        event.preventDefault();
        moveFocus(6 - ((focusedDate.getDay() + 6) % 7));
        break;
      case "PageUp":
        event.preventDefault();
        shiftMonth(-1);
        break;
      case "PageDown":
        event.preventDefault();
        shiftMonth(1);
        break;
      case "Enter":
      case " ":
        event.preventDefault();
        commit(focusedDate);
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
        className={`select-trigger date-range-trigger${className ? ` ${className}` : ""}`}
        ref={triggerRef}
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label="选择日期范围"
        title="选择日期范围"
        disabled={disabled}
        onClick={() => (open ? closePanel(false) : openPanel())}
      >
        <span className="select-value">
          {text ? text : <span className="select-placeholder">全部时间</span>}
        </span>
        <CalendarDays className="select-chevron" aria-hidden="true" />
      </button>

      {open
        ? createPortal(
            <div
              className="date-panel"
              role="dialog"
              aria-label="选择日期范围"
              tabIndex={-1}
              ref={panelRef}
              data-escape-scope="local"
              style={{
                top: position?.top ?? -9999,
                left: position?.left ?? -9999,
                width: position?.width,
                visibility: position ? "visible" : "hidden",
              }}
              onKeyDown={onPanelKeyDown}
            >
              <div className="date-panel-head">
                <button
                  className="date-nav"
                  type="button"
                  aria-label={view === "days" ? "上一月" : "上一年"}
                  onClick={() => (view === "days" ? shiftMonth(-1) : setViewMonth(new Date(viewMonth.getFullYear() - 1, viewMonth.getMonth(), 1)))}
                >
                  <ChevronLeft aria-hidden="true" />
                </button>
                <button
                  className="date-title"
                  type="button"
                  aria-label={view === "days" ? "切换到月份选择" : "切换到日期选择"}
                  onClick={() => setView(view === "days" ? "months" : "days")}
                >
                  {view === "days" ? formatMonthLabel(viewMonth) : formatYearLabel(viewMonth)}
                </button>
                <button
                  className="date-nav"
                  type="button"
                  aria-label={view === "days" ? "下一月" : "下一年"}
                  onClick={() => (view === "days" ? shiftMonth(1) : setViewMonth(new Date(viewMonth.getFullYear() + 1, viewMonth.getMonth(), 1)))}
                >
                  <ChevronRight aria-hidden="true" />
                </button>
              </div>

              {view === "days" ? (
                <>
                  <div className="date-weekdays" aria-hidden="true">
                    {weekdayHeaders.map((name) => (
                      <span key={name}>{name}</span>
                    ))}
                  </div>
                  <div className="date-grid">
                    {monthGrid(viewMonth).map((date) => {
                      const iso = toISODate(date);
                      const outside = date.getMonth() !== viewMonth.getMonth();
                      const isEdge = iso === from || iso === to;
                      const pending = pendingAnchor !== null && isSameDay(date, pendingAnchor);
                      const inRange = Boolean(from && to && iso > from && iso < to);
                      const isToday = isSameDay(date, today);
                      const classes = [
                        "date-cell",
                        outside ? "is-outside" : "",
                        isToday ? "is-today" : "",
                        inRange ? "is-in-range" : "",
                        isEdge ? "is-edge" : "",
                        pending ? "is-pending" : "",
                      ]
                        .filter(Boolean)
                        .join(" ");
                      return (
                        <button
                          key={iso}
                          type="button"
                          className={classes}
                          data-date={iso}
                          aria-label={formatFullDate(date)}
                          aria-current={isToday ? "date" : undefined}
                          tabIndex={isSameDay(date, focusedDate) ? 0 : -1}
                          onClick={() => commit(date)}
                          onFocus={() => setFocusedDate(date)}
                        >
                          {date.getDate()}
                        </button>
                      );
                    })}
                  </div>
                </>
              ) : (
                <div className="date-month-grid">
                  {MONTHS.map((month) => {
                    const active = viewMonth.getMonth() === month;
                    return (
                      <button
                        key={month}
                        type="button"
                        className={active ? "date-month is-active" : "date-month"}
                        onClick={() => {
                          setViewMonth(new Date(viewMonth.getFullYear(), month, 1));
                          setView("days");
                        }}
                      >
                        {month + 1}月
                      </button>
                    );
                  })}
                </div>
              )}

              <div className="date-panel-foot">
                <button className="text-action" type="button" onClick={() => commit(today)}>
                  今天
                </button>
                <button
                  className="text-action"
                  type="button"
                  disabled={!from && !to}
                  onClick={() => {
                    onChange("", "");
                    closePanel();
                  }}
                >
                  清除
                </button>
              </div>
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
