import { useState } from "react";
import { SlidersHorizontal } from "lucide-react";
import { InlineError, SaveBar, SectionTitle } from "../../components/ConfigControls";
import {
  authSchemeLabel,
  isProviderDraftDirty,
  protocolLabel,
  type ProviderDraft,
} from "./providerModel";

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
  const set = (patch: Partial<ProviderDraft>) => onChange({ ...draft, ...patch });
  const dirty = isProviderDraftDirty(draft, savedDraft);

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
          <label className="field">
            <span>上游地址</span>
            <input value={draft.baseUrl} onChange={(event) => set({ baseUrl: event.target.value })} placeholder="https://api.example.com/v1" spellCheck={false} />
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
          <label className="field">
            <span>鉴权方式</span>
            <select value={draft.authScheme} onChange={(event) => set({ authScheme: event.target.value as ProviderDraft["authScheme"] })}>
              <option value="bearer">{authSchemeLabel.bearer}</option>
              <option value="x-api-key">{authSchemeLabel["x-api-key"]}</option>
              <option value="x-goog-api-key">{authSchemeLabel["x-goog-api-key"]}</option>
            </select>
          </label>
          <label className="field">
            <span>协议</span>
            {draft.id === null ? (
              <select value={draft.protocol} onChange={(event) => set({ protocol: event.target.value as ProviderDraft["protocol"] })}>
                <option value="openai">{protocolLabel.openai}</option>
                <option value="anthropic">{protocolLabel.anthropic}</option>
                <option value="responses">{protocolLabel.responses}</option>
                <option value="gemini">{protocolLabel.gemini}</option>
              </select>
            ) : (
              <span className="field-static">{protocolLabel[draft.protocol]}</span>
            )}
          </label>
          <label className="field field-wide">
            <span>额外请求头</span>
            <textarea
              rows={2}
              value={draft.extraHeadersText}
              onChange={(event) => set({ extraHeadersText: event.target.value })}
              placeholder={"每行一条，例如\nX-Trace: 1"}
              spellCheck={false}
            />
          </label>
        </div>
      </section>

      {error ? <InlineError message={error} /> : null}
      <SaveBar dirty={dirty} busy={busy} label={draft.id ? "保存上游" : "登记上游"} onDiscard={onDiscard} />
    </form>
  );
}
