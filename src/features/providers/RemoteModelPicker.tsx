import { useEffect, useMemo, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { Download } from "lucide-react";
import { useAnchoredPanel } from "../../hooks/useAnchoredPanel";
import { listRemoteModels, type RemoteModel } from "../../services/telemetry";
import type { ProviderEndpoint } from "../../services/config";
import { protocolLabel } from "./providerModel";

/**
 * 从上游 `/models` 拉取可用模型并回填。触发按钮挂在「上游模型名」输入旁，
 * 面板 portal 到 body（复用模型选择器的外观），可切协议端点、搜索、点选单个。
 */
export function RemoteModelPicker({
  providerId,
  endpoints,
  disabled,
  onPick,
}: {
  providerId: string;
  endpoints: ProviderEndpoint[];
  disabled?: boolean;
  onPick: (model: RemoteModel) => void;
}) {
  const [open, setOpen] = useState(false);
  const [protocol, setProtocol] = useState("");
  const [models, setModels] = useState<RemoteModel[] | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState("");
  const triggerRef = useRef<HTMLButtonElement>(null);
  const panelRef = useRef<HTMLDivElement>(null);

  const activeProtocol = protocol || endpoints[0]?.protocol || "";
  const position = useAnchoredPanel({ open, triggerRef, panelRef, width: "content" });

  useEffect(() => {
    if (!open) return;
    let alive = true;
    setLoading(true);
    setError(null);
    setModels(null);
    listRemoteModels(providerId, activeProtocol || null)
      .then((result) => {
        if (alive) setModels(result);
      })
      .catch((err) => {
        if (alive) setError(String(err));
      })
      .finally(() => {
        if (alive) setLoading(false);
      });
    return () => {
      alive = false;
    };
  }, [open, providerId, activeProtocol]);

  useEffect(() => {
    if (!open) return;
    setQuery("");
    panelRef.current?.querySelector<HTMLInputElement>(".model-picker-search")?.focus();

    const onPointerDown = (event: PointerEvent) => {
      const target = event.target as Node;
      if (panelRef.current?.contains(target) || triggerRef.current?.contains(target)) return;
      setOpen(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    return () => document.removeEventListener("pointerdown", onPointerDown);
  }, [open]);

  const filtered = useMemo(() => {
    const list = models ?? [];
    const needle = query.trim().toLowerCase();
    if (!needle) return list;
    return list.filter((model) =>
      `${model.id} ${model.displayName ?? ""}`.toLowerCase().includes(needle),
    );
  }, [models, query]);

  const pick = (model: RemoteModel) => {
    onPick(model);
    setOpen(false);
  };

  return (
    <>
      <button
        className="glyph-button"
        type="button"
        ref={triggerRef}
        title="从上游拉取模型"
        aria-label="从上游拉取模型"
        aria-haspopup="dialog"
        aria-expanded={open}
        disabled={disabled || endpoints.length === 0}
        onClick={() => setOpen((value) => !value)}
      >
        <Download aria-hidden="true" />
      </button>

      {open
        ? createPortal(
            <div
              className="model-picker"
              role="dialog"
              aria-label="从上游拉取模型"
              ref={panelRef}
              data-escape-scope="local"
              style={{ top: position?.top ?? -9999, left: position?.left ?? -9999 }}
              onKeyDown={(event) => {
                if (event.key !== "Escape") return;
                event.stopPropagation();
                setOpen(false);
                triggerRef.current?.focus();
              }}
            >
              <div className="remote-picker-head">
                {endpoints.length > 1 ? (
                  <div className="remote-picker-protocols" role="group" aria-label="拉取所用端点">
                    {endpoints.map((endpoint) => (
                      <button
                        key={endpoint.protocol}
                        type="button"
                        className={
                          endpoint.protocol === activeProtocol
                            ? "quiet-button is-primary"
                            : "quiet-button"
                        }
                        onClick={() => setProtocol(endpoint.protocol)}
                      >
                        {protocolLabel[endpoint.protocol] ?? endpoint.protocol}
                      </button>
                    ))}
                  </div>
                ) : null}
                <input
                  className="model-picker-search"
                  type="search"
                  value={query}
                  placeholder="搜索模型…"
                  spellCheck={false}
                  onChange={(event) => setQuery(event.target.value)}
                />
              </div>
              <div className="model-picker-list">
                {loading ? (
                  <p className="model-picker-empty">正在读取上游模型…</p>
                ) : error ? (
                  <p className="model-picker-empty">{error}</p>
                ) : filtered.length === 0 ? (
                  <p className="model-picker-empty">
                    {models && models.length === 0 ? "上游没有返回模型。" : "没有匹配的模型。"}
                  </p>
                ) : (
                  filtered.map((model) => (
                    <button
                      className="model-picker-cell"
                      type="button"
                      key={model.id}
                      onClick={() => pick(model)}
                    >
                      <span className="model-picker-name">{model.displayName ?? model.id}</span>
                      {model.displayName ? (
                        <code className="model-picker-id">{model.id}</code>
                      ) : null}
                    </button>
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
