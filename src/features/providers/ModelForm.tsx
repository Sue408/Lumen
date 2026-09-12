import { FormActions, InlineError, TogglePill } from "../../components/ConfigControls";
import { capabilityIcons } from "./capabilityIcons";
import { capabilityLabel, capabilityOrder, type ModelDraft } from "./providerModel";

export function ModelForm({
  draft,
  busy,
  error,
  onChange,
  onSubmit,
  onCancel,
}: {
  draft: ModelDraft;
  busy: boolean;
  error: string | null;
  onChange: (draft: ModelDraft) => void;
  onSubmit: () => void;
  onCancel: () => void;
}) {
  const set = (patch: Partial<ModelDraft>) => onChange({ ...draft, ...patch });

  return (
    <form
      className="entry-form"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <div className="field-grid">
        <label className="field">
          <span>上游模型名</span>
          <input value={draft.modelId} onChange={(event) => set({ modelId: event.target.value })} placeholder="gpt-4o" spellCheck={false} autoFocus />
        </label>
        <label className="field">
          <span>显示名</span>
          <input value={draft.displayName} onChange={(event) => set({ displayName: event.target.value })} placeholder="留空则同上" />
        </label>
        <label className="field">
          <span>输入单价 · $ / 百万</span>
          <input inputMode="decimal" value={draft.inputPrice} onChange={(event) => set({ inputPrice: event.target.value })} />
        </label>
        <label className="field">
          <span>输出单价 · $ / 百万</span>
          <input inputMode="decimal" value={draft.outputPrice} onChange={(event) => set({ outputPrice: event.target.value })} />
        </label>
        <label className="field">
          <span>缓存读单价 · $ / 百万</span>
          <input inputMode="decimal" value={draft.cacheReadPrice} onChange={(event) => set({ cacheReadPrice: event.target.value })} />
        </label>
        <label className="field">
          <span>缓存写单价 · $ / 百万</span>
          <input inputMode="decimal" value={draft.cacheCreationPrice} onChange={(event) => set({ cacheCreationPrice: event.target.value })} />
        </label>
        <label className="field">
          <span>上下文长度 · Tokens</span>
          <input inputMode="numeric" value={draft.contextWindow} placeholder="128000" onChange={(event) => set({ contextWindow: event.target.value })} />
        </label>
        <div className="field field-wide">
          <span>能力标签</span>
          <div className="capability-group" role="group" aria-label="模型能力">
            {capabilityOrder.map((id) => {
              const Icon = capabilityIcons[id];
              const active = draft.capabilities.includes(id);
              return (
                <button
                  key={id}
                  className={active ? "capability-pill is-on" : "capability-pill"}
                  type="button"
                  role="switch"
                  aria-checked={active}
                  disabled={busy}
                  onClick={() =>
                    set({
                      capabilities: active
                        ? draft.capabilities.filter((item) => item !== id)
                        : [...draft.capabilities, id],
                    })
                  }
                >
                  <Icon aria-hidden="true" />
                  {capabilityLabel[id]}
                </button>
              );
            })}
          </div>
        </div>
        <div className="field-inline">
          <TogglePill checked={draft.enabled} label="启用该模型" disabled={busy} onChange={(next) => set({ enabled: next })} />
        </div>
      </div>
      {error ? <InlineError message={error} /> : null}
      <FormActions busy={busy} submitLabel={draft.id ? "保存模型" : "登记模型"} onCancel={onCancel} />
    </form>
  );
}
