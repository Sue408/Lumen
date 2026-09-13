import { useState } from "react";
import { Modal } from "../../components/Modal";
import { FormActions, InlineError } from "../../components/ConfigControls";

export type ProviderBasicsValues = { name: string; apiKey: string };

/**
 * 提供商名称与密钥的编辑弹窗。主视图不再常驻这两个输入，改由这里集中修改；
 * 结果回填到页面草稿，仍由底部 SaveBar 统一落库。
 */
export function ProviderBasicsModal({
  initial,
  isNew,
  busy,
  error,
  onSubmit,
  onClose,
}: {
  initial: ProviderBasicsValues;
  isNew: boolean;
  busy: boolean;
  error: string | null;
  onSubmit: (values: ProviderBasicsValues) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(initial);
  const [reveal, setReveal] = useState(false);
  const set = (patch: Partial<ProviderBasicsValues>) => setDraft({ ...draft, ...patch });

  return (
    <Modal title={isNew ? "填写基础信息" : "编辑基础信息"} onClose={onClose}>
      <form
        className="entry-form"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit({ name: draft.name.trim(), apiKey: draft.apiKey });
        }}
      >
        <div className="field-grid">
          <label className="field field-wide">
            <span>名称</span>
            <input
              value={draft.name}
              onChange={(event) => set({ name: event.target.value })}
              placeholder="例如 DeepSeek"
              autoFocus
            />
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
        {error ? <InlineError message={error} /> : null}
        <FormActions busy={busy} submitLabel="确定" onCancel={onClose} />
      </form>
    </Modal>
  );
}
