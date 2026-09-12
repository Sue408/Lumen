import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Info } from "lucide-react";
import { protocolEndpoints } from "./protocolEndpoints";

const PANEL_ID = "protocol-help-popover";

/**
 * 协议调用说明：一个统一的信息图标，悬停 / 聚焦时在旁侧弹出各协议的入站地址。
 * 浮层 portal 到 body 并用 fixed 定位，避免被 workbench-sheet 的 overflow 裁切
 * （与品牌图标选择器同一套定位思路）。
 */
export function ProtocolHelp() {
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);
  const [open, setOpen] = useState(false);
  const [position, setPosition] = useState<{ top: number; left: number } | null>(null);

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
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("keydown", onKeyDown);
    return () => document.removeEventListener("keydown", onKeyDown);
  }, [open]);

  return (
    <>
      <button
        className="protocol-help-trigger"
        type="button"
        ref={triggerRef}
        aria-label="查看各协议的调用地址"
        aria-describedby={open ? PANEL_ID : undefined}
        onMouseEnter={() => setOpen(true)}
        onMouseLeave={() => setOpen(false)}
        onFocus={() => setOpen(true)}
        onBlur={() => setOpen(false)}
      >
        <Info aria-hidden="true" />
      </button>

      {open
        ? createPortal(
            <div
              className="protocol-help"
              id={PANEL_ID}
              role="tooltip"
              ref={panelRef}
              style={{
                top: position?.top ?? -9999,
                left: position?.left ?? -9999,
                visibility: position ? "visible" : "hidden",
              }}
            >
              <p className="protocol-help-title">各协议的调用地址</p>
              <dl className="protocol-help-list">
                {protocolEndpoints.map((endpoint) => (
                  <div className="protocol-help-row" key={endpoint.protocol}>
                    <dt>{endpoint.label}</dt>
                    <dd>
                      <code>
                        {endpoint.method} {endpoint.path}
                      </code>
                      {endpoint.note ? <em>{endpoint.note}</em> : null}
                    </dd>
                  </div>
                ))}
              </dl>
              <p className="protocol-help-foot">
                基址 http://127.0.0.1:{"{端口}"}，调用需携带虚拟密钥。
              </p>
            </div>,
            document.body,
          )
        : null}
    </>
  );
}
