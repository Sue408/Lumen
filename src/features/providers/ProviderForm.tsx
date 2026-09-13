import { useState, type ReactNode } from "react";
import { ChevronDown, Network, Pencil, Plus, Repeat, Trash2 } from "lucide-react";
import {
  GlyphButton,
  InlineError,
  SaveBar,
  SectionTitle,
  StatusDot,
  TogglePill,
} from "../../components/ConfigControls";
import {
  authSchemeLabel,
  draftHeaderRules,
  endpointHost,
  isProviderDraftDirty,
  nextEndpointDraft,
  protocolLabel,
  protocolOrder,
  type EndpointDraft,
  type ProviderDraft,
} from "./providerModel";
import { summarizeProviderHeaderRules } from "./providerHeaderModel.ts";
import { HeaderRulesHelp } from "./HeaderRulesHelp";

export function ProviderForm({
  draft,
  savedDraft,
  busy,
  error,
  onChange,
  onDiscard,
  onSubmit,
  children,
}: {
  draft: ProviderDraft;
  savedDraft: ProviderDraft;
  busy: boolean;
  error: string | null;
  onChange: (draft: ProviderDraft) => void;
  onDiscard: () => void;
  onSubmit: () => void;
  /** 模型区插槽：置于协议端点与请求头映射之间，让核心内容更靠前。 */
  children?: ReactNode;
}) {
  const [rulesOpen, setRulesOpen] = useState(false);
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const set = (patch: Partial<ProviderDraft>) => onChange({ ...draft, ...patch });
  const dirty = isProviderDraftDirty(draft, savedDraft);

  const setEndpoint = (index: number, patch: Partial<EndpointDraft>) =>
    set({
      endpoints: draft.endpoints.map((endpoint, current) =>
        current === index ? { ...endpoint, ...patch } : endpoint,
      ),
    });
  const addEndpoint = () => {
    set({ endpoints: [...draft.endpoints, nextEndpointDraft(draft.endpoints)] });
    setEditingIndex(draft.endpoints.length);
  };
  const removeEndpoint = (index: number) => {
    set({ endpoints: draft.endpoints.filter((_, current) => current !== index) });
    setEditingIndex((previous) => {
      if (previous === null || previous === index) return null;
      return previous > index ? previous - 1 : previous;
    });
  };

  return (
    <form
      className="entry-form"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <section className="sheet-section">
        <SectionTitle
          icon={<Network aria-hidden="true" />}
          action={
            <button className="text-action" type="button" onClick={addEndpoint} disabled={busy}>
              <Plus aria-hidden="true" />
              添加协议端点
            </button>
          }
        >
          协议端点
        </SectionTitle>
        <div className="endpoint-list">
          {draft.endpoints.map((endpoint, index) => {
            const editing = editingIndex === index;
            return (
              <div
                className={editing ? "endpoint-item is-editing" : "endpoint-item"}
                key={endpoint.id ?? `new-${index}`}
              >
                <div className="endpoint-item-head">
                  <StatusDot alive={endpoint.enabled} />
                  <span className="endpoint-item-protocol">{protocolLabel[endpoint.protocol]}</span>
                  <code className="endpoint-item-host" title={endpoint.baseUrl}>
                    {endpointHost(endpoint.baseUrl)}
                  </code>
                  <div className="endpoint-item-actions">
                    <GlyphButton
                      label={editing ? "收起该端点" : "编辑该端点"}
                      disabled={busy}
                      onClick={() => setEditingIndex(editing ? null : index)}
                    >
                      <Pencil aria-hidden="true" />
                    </GlyphButton>
                    <GlyphButton
                      label="删除该端点"
                      danger
                      disabled={busy || draft.endpoints.length <= 1}
                      onClick={() => removeEndpoint(index)}
                    >
                      <Trash2 aria-hidden="true" />
                    </GlyphButton>
                  </div>
                </div>
                {editing ? (
                  <div className="endpoint-item-body">
                    <label className="field">
                      <span>协议</span>
                      <select
                        value={endpoint.protocol}
                        onChange={(event) =>
                          setEndpoint(index, {
                            protocol: event.target.value as EndpointDraft["protocol"],
                          })
                        }
                      >
                        {protocolOrder.map((protocol) => (
                          <option key={protocol} value={protocol}>
                            {protocolLabel[protocol]}
                          </option>
                        ))}
                      </select>
                    </label>
                    <label className="field">
                      <span>上游地址</span>
                      <input
                        value={endpoint.baseUrl}
                        onChange={(event) => setEndpoint(index, { baseUrl: event.target.value })}
                        placeholder="https://api.example.com/v1"
                        spellCheck={false}
                      />
                    </label>
                    <label className="field endpoint-auth">
                      <span>鉴权方式</span>
                      <select
                        value={endpoint.authScheme}
                        onChange={(event) =>
                          setEndpoint(index, {
                            authScheme: event.target.value as EndpointDraft["authScheme"],
                          })
                        }
                      >
                        <option value="bearer">{authSchemeLabel.bearer}</option>
                        <option value="x-api-key">{authSchemeLabel["x-api-key"]}</option>
                        <option value="x-goog-api-key">{authSchemeLabel["x-goog-api-key"]}</option>
                      </select>
                    </label>
                    <div className="endpoint-item-foot">
                      <TogglePill
                        small
                        checked={endpoint.enabled}
                        label={endpoint.enabled ? "停用该协议端点" : "启用该协议端点"}
                        disabled={busy}
                        onChange={(next) => setEndpoint(index, { enabled: next })}
                      />
                      <button
                        className="text-action"
                        type="button"
                        disabled={busy}
                        onClick={() => setEditingIndex(null)}
                      >
                        收起
                      </button>
                    </div>
                  </div>
                ) : null}
              </div>
            );
          })}
        </div>
        <p className="field-hint">
          一个提供商可挂多个协议端点，模型在多协议间共享；同一模型可被不同协议的路由复用。
        </p>
      </section>

      {children}

      <section className="sheet-section">
        <SectionTitle icon={<Repeat aria-hidden="true" />}>
          请求头映射
          <HeaderRulesHelp />
        </SectionTitle>
        <button
          className="header-rules-toggle"
          type="button"
          aria-expanded={rulesOpen}
          onClick={() => setRulesOpen((value) => !value)}
        >
          <ChevronDown className="header-rules-chevron" aria-hidden="true" />
          <span>{rulesOpen ? "收起规则" : "配置规则"}</span>
          <span className="header-rules-summary">
            {summarizeProviderHeaderRules(draftHeaderRules(draft))}
          </span>
        </button>
        {rulesOpen ? (
          <div className="field-grid header-rules-panel">
            <label className="field field-wide">
              <span>透传（每行一个，支持 * 通配）</span>
              <textarea
                rows={2}
                value={draft.forwardText}
                onChange={(event) => set({ forwardText: event.target.value })}
                placeholder={"非 x- 前缀的头也能放行，例如\nsession_id"}
                spellCheck={false}
              />
            </label>
            <label className="field field-wide">
              <span>替换（每行 `来源 → 目标`）</span>
              <textarea
                rows={2}
                value={draft.replaceText}
                onChange={(event) => set({ replaceText: event.target.value })}
                placeholder={"session_id → x-opencode-session"}
                spellCheck={false}
              />
            </label>
            <label className="field field-wide">
              <span>添加（常量，每行 `name: value`）</span>
              <textarea
                rows={2}
                value={draft.extraHeadersText}
                onChange={(event) => set({ extraHeadersText: event.target.value })}
                placeholder={"user-agent: opencode/local"}
                spellCheck={false}
              />
            </label>
            <label className="field field-wide">
              <span>移除（每行一个，支持 * 通配）</span>
              <textarea
                rows={2}
                value={draft.removeText}
                onChange={(event) => set({ removeText: event.target.value })}
                placeholder={"x-internal*"}
                spellCheck={false}
              />
            </label>
          </div>
        ) : null}
      </section>

      {error ? <InlineError message={error} /> : null}
      <SaveBar dirty={dirty} busy={busy} label={draft.id ? "保存上游" : "登记上游"} onDiscard={onDiscard} />
    </form>
  );
}
