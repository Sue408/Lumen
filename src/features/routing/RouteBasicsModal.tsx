import { useState } from "react";
import { Modal } from "../../components/Modal";
import { FormActions, InlineError } from "../../components/ConfigControls";

export type RouteBasicsDraft = {
  id: string | null;
  alias: string;
  displayName: string;
};

/** 路由基础信息（别名 / 展示名）走弹窗；协议隐式，对所有入站协议开放。 */
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
        </div>
        {error ? <InlineError message={error} /> : null}
        <FormActions busy={busy} submitLabel={isNew ? "创建路由" : "保存"} onCancel={onClose} />
      </form>
    </Modal>
  );
}
