import { useLayoutEffect, useRef } from "react";

function snapshot(container: HTMLElement) {
  const positions = new Map<string, DOMRect>();
  container.querySelectorAll<HTMLElement>("[data-flip-key]").forEach((element) => {
    const key = element.dataset.flipKey;
    if (key) positions.set(key, element.getBoundingClientRect());
  });
  return positions;
}

/**
 * FLIP reorder helper. Call `capture()` immediately before mutating the list,
 * then let the layout effect animate every row from its old position to the new
 * one. Rows are tracked by their `data-flip-key`; no animation library needed.
 */
export function useFlipList<T extends HTMLElement>() {
  const containerRef = useRef<T>(null);
  const positionsRef = useRef<Map<string, DOMRect> | null>(null);

  const capture = () => {
    const container = containerRef.current;
    if (container) positionsRef.current = snapshot(container);
  };

  useLayoutEffect(() => {
    const container = containerRef.current;
    const previous = positionsRef.current;
    if (!container || !previous) return;
    positionsRef.current = null;

    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    if (reduce) return;

    container.querySelectorAll<HTMLElement>("[data-flip-key]").forEach((element) => {
      const key = element.dataset.flipKey;
      if (!key) return;
      const before = previous.get(key);
      if (!before) return;
      const after = element.getBoundingClientRect();
      const dx = before.left - after.left;
      const dy = before.top - after.top;
      if (Math.abs(dx) < 1 && Math.abs(dy) < 1) return;
      element.animate(
        [{ transform: `translate(${dx}px, ${dy}px)` }, { transform: "none" }],
        { duration: 200, easing: "cubic-bezier(0.2, 0, 0, 1)" },
      );
    });
  });

  return { containerRef, capture };
}
