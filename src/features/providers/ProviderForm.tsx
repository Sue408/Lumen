import { useState } from "react";
import { ChevronDown, Network, Plus, Repeat, SlidersHorizontal, Trash2 } from "lucide-react";
import { InlineError, SaveBar, SectionTitle, TogglePill } from "../../components/ConfigControls";
import {
  authSchemeLabel,
  draftHeaderRules,
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
}: {
  draft: ProviderDraft;
  savedDraft: ProviderDraft;
  busy: boolean;
  error: string | null;
  onChange: (draft: ProviderDraft) => void;
  onDiscard: () => void;
  onSubmit: () => void;
}) {
  const [reveal, setReveal] = useState(false);
  const [rulesOpen, setRulesOpen] = useState(false);
  const set = (patch: Partial<ProviderDraft>) => onChange({ ...draft, ...patch });
  const dirty = isProviderDraftDirty(draft, savedDraft);

  const setEndpoint = (index: number, patch: Partial<EndpointDraft>) =>
    set({
      endpoints: draft.endpoints.map((endpoint, current) =>
        current === index ? { ...endpoint, ...patch } : endpoint,
      ),
    });
  const addEndpoint = () =>
    set({ endpoints: [...draft.endpoints, nextEndpointDraft(draft.endpoints)] });
  const removeEndpoint = (index: number) =>
    set({ endpoints: draft.endpoints.filter((_, current) => current !== index) });

  return (
    <form
      className="entry-form"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <section className="sheet-section">
        <SectionTitle icon={<SlidersHorizontal aria-hidden="true" />}>基础信息</SectionTitle>
        <div className="field-grid">
          <label className="field">
            <span>名称</span>
            <input value={draft.name} onChange={(event) => set({ name: event.target.value })} placeholder="例如 DeepSeek" />
          </label>
          <label className="field field-wide">
            <span>API Key</span>
            <span className="key-field">
              <input
                type={reveal ? "text" : "password"}
                value={draft.apiKey}
                onChange={(event) => set({ apiKey: event.target.value })}
                placeholder="sk-…"
                spellCheck={false}
                autoComplete="off"
              />
              <button className="text-action" type="button" onClick={() => setReveal((value) => !value)}>
                {reveal ? "隐藏" : "显示"}
              </button>
            </span>
          </label>
        </div>
      </section>

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
          {draft.endpoints.map((endpoint, index) => (
            <div className="endpoint-row" key={endpoint.id ?? `new-${index}`}>
              <label className="field">
                <span>协议</span>
                <select
                  value={endpoint.protocol}
                  onChange={(event) =>
                    setEndpoint(index, { protocol: event.target.value as EndpointDraft["protocol"] })
                  }
                >
                  {protocolOrder.map((protocol) => (
                    <option key={protocol} value={protocol}>
                      {protocolLabel[protocol]}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field endpoint-url">
                <span>上游地址</span>
                <input
                  value={endpoint.baseUrl}
                  onChange={(event) => setEndpoint(index, { baseUrl: event.target.value })}
                  placeholder="https://api.example.com/v1"
                  spellCheck={false}
                />
              </label>
              <label className="field">
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
              <div className="endpoint-actions">
                <TogglePill
                  small
                  checked={endpoint.enabled}
                  label={endpoint.enabled ? "停用该协议端点" : "启用该协议端点"}
                  disabled={busy}
                  onChange={(next) => setEndpoint(index, { enabled: next })}
                />
                <button
                  className="text-action is-icon"
                  type="button"
                  aria-label="删除该协议端点"
                  disabled={busy || draft.endpoints.length <= 1}
                  onClick={() => removeEndpoint(index)}
                >
                  <Trash2 aria-hidden="true" />
                </button>
              </div>
            </div>
          ))}
        </div>
        <p className="field-hint">
          一个提供商可挂多个协议端点，模型在多协议间共享；同一模型可被不同协议的路由复用。
        </p>
      </section>

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
