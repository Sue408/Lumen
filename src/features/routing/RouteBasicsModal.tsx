import { useState } from "react";
import { Modal } from "../../components/Modal";
import { FormActions, InlineError } from "../../components/ConfigControls";
import { protocolLabel, type Protocol } from "../../services/config";

export type RouteBasicsDraft = {
  id: string | null;
  alias: string;
  displayName: string;
  protocol: Protocol;
};

/** 路由基础信息（别名 / 展示名 / 协议）走弹窗；协议创建后锁定，编辑态只读。 */
export function RouteBasicsModal({
  initial,
  busy,
  error,
  onSubmit,
  onClose,
}: {
  initial: RouteBasicsDraft;
  busy: boolean;
  error: string | null;
  onSubmit: (draft: RouteBasicsDraft) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(initial);
  const isNew = initial.id === null;
  const set = (patch: Partial<RouteBasicsDraft>) => setDraft({ ...draft, ...patch });

  return (
    <Modal title={isNew ? "新建路由" : "编辑基础信息"} onClose={onClose}>
      <form
        className="entry-form"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit({ ...draft, alias: draft.alias.trim(), displayName: draft.displayName.trim() });
        }}
      >
        <div className="field-grid">
          <label className="field">
            <span>路由别名</span>
            <input
              value={draft.alias}
              onChange={(event) => set({ alias: event.target.value })}
              placeholder="deepseek/deepseek-v4-flash"
              spellCheck={false}
              autoFocus
            />
          </label>
          <label className="field">
            <span>展示名</span>
            <input
              value={draft.displayName}
              onChange={(event) => set({ displayName: event.target.value })}
              placeholder="留空则同别名"
            />
          </label>
          <label className="field">
            <span>协议</span>
            {isNew ? (
              <select
                value={draft.protocol}
                onChange={(event) => set({ protocol: event.target.value as Protocol })}
              >
                <option value="openai">{protocolLabel.openai}</option>
                <option value="anthropic">{protocolLabel.anthropic}</option>
                <option value="responses">{protocolLabel.responses}</option>
                <option value="gemini">{protocolLabel.gemini}</option>
              </select>
            ) : (
              <span className="field-static">{protocolLabel[draft.protocol]}</span>
            )}
          </label>
        </div>
        {error ? <InlineError message={error} /> : null}
        <FormActions busy={busy} submitLabel={isNew ? "创建路由" : "保存"} onCancel={onClose} />
      </form>
    </Modal>
  );
}
