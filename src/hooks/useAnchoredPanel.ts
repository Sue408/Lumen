import { useLayoutEffect, useState, type RefObject } from "react";

export type AnchoredPosition = { top: number; left: number; width: number };

/**
 * 把浮层锚定在触发元素上的共享定位：默认落在下方，放不下翻到上方，
 * 左右越界时收边回到视口内。portal 浮层（下拉 / 日期面板）复用它，
 * 避免各自重复量宽高。打开当帧在 layout 阶段测量，不会闪过旧位置。
 */
export function useAnchoredPanel({
  open,
  triggerRef,
  panelRef,
  width = "match",
  gap = 4,
  margin = 8,
  deps = [],
}: {
  open: boolean;
  triggerRef: RefObject<HTMLElement | null>;
  panelRef: RefObject<HTMLElement | null>;
  /** "match"：面板宽取触发与内容较大者；"content"：仅取内容宽。 */
  width?: "match" | "content";
  gap?: number;
  margin?: number;
  /** 面板尺寸随调用方数据变化时，提供附加依赖以重新定位。 */
  deps?: readonly unknown[];
}): AnchoredPosition | null {
  const [position, setPosition] = useState<AnchoredPosition | null>(null);

  useLayoutEffect(() => {
    if (!open) return;
    const trigger = triggerRef.current;
    const panel = panelRef.current;
    if (!trigger || !panel) return;

    const place = () => {
      const rect = trigger.getBoundingClientRect();
      const panelRect = panel.getBoundingClientRect();
      const panelWidth =
        width === "match" ? Math.max(rect.width, panelRect.width) : panelRect.width;

      let top = rect.bottom + gap;
      if (
        top + panelRect.height > window.innerHeight - margin &&
        rect.top - gap - panelRect.height >= margin
      ) {
        top = rect.top - gap - panelRect.height;
      }

      let left = rect.left;
      if (left + panelWidth > window.innerWidth - margin) {
        left = window.innerWidth - margin - panelWidth;
      }
      if (left < margin) left = margin;

      setPosition({ top, left, width: panelWidth });
    };

    place();
    window.addEventListener("resize", place);
    window.addEventListener("scroll", place, true);
    return () => {
      window.removeEventListener("resize", place);
      window.removeEventListener("scroll", place, true);
    };
    // deps 由调用方保证长度稳定（面板尺寸依赖）。
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [open, width, gap, margin, ...deps]);

  return position;
}
