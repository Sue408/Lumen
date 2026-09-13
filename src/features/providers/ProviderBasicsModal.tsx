import { useState } from "react";
import { Modal } from "../../components/Modal";
import { FormActions, InlineError } from "../../components/ConfigControls";
import { authSchemeLabel, protocolLabel, protocolOrder } from "./providerModel";
import type { AuthScheme, Protocol } from "../../services/config";

export type ProviderBasicsValues = {
  name: string;
  apiKey: string;
  protocol: Protocol;
  baseUrl: string;
  authScheme: AuthScheme;
};

/**
 * 提供商名称与密钥的编辑弹窗。主视图不再常驻这些输入。
 * 新建时额外收集首个协议端点（创建必须至少一条），编辑时只改名称与密钥。
 */
export function ProviderBasicsModal({
  mode,
  initial,
  busy,
  error,
  onSubmit,
  onClose,
}: {
  mode: "create" | "edit";
  initial: ProviderBasicsValues;
  busy: boolean;
  error: string | null;
  onSubmit: (values: ProviderBasicsValues) => void;
  onClose: () => void;
}) {
  const [draft, setDraft] = useState(initial);
  const [reveal, setReveal] = useState(false);
  const set = (patch: Partial<ProviderBasicsValues>) => setDraft({ ...draft, ...patch });
  const isCreate = mode === "create";

  return (
    <Modal title={isCreate ? "登记上游" : "编辑基础信息"} onClose={onClose}>
      <form
        className="entry-form"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit({ ...draft, name: draft.name.trim(), baseUrl: draft.baseUrl.trim() });
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
          {isCreate ? (
            <>
              <label className="field">
                <span>协议</span>
                <select
                  value={draft.protocol}
                  onChange={(event) => set({ protocol: event.target.value as Protocol })}
                >
                  {protocolOrder.map((protocol) => (
                    <option key={protocol} value={protocol}>
                      {protocolLabel[protocol]}
                    </option>
                  ))}
                </select>
              </label>
              <label className="field">
                <span>鉴权方式</span>
                <select
                  value={draft.authScheme}
                  onChange={(event) => set({ authScheme: event.target.value as AuthScheme })}
                >
                  <option value="bearer">{authSchemeLabel.bearer}</option>
                  <option value="x-api-key">{authSchemeLabel["x-api-key"]}</option>
                  <option value="x-goog-api-key">{authSchemeLabel["x-goog-api-key"]}</option>
                </select>
              </label>
              <label className="field field-wide">
                <span>上游地址</span>
                <input
                  value={draft.baseUrl}
                  onChange={(event) => set({ baseUrl: event.target.value })}
                  placeholder="https://api.example.com/v1"
                  spellCheck={false}
                />
              </label>
            </>
          ) : null}
        </div>
        {error ? <InlineError message={error} /> : null}
        <FormActions busy={busy} submitLabel={isCreate ? "登记" : "确定"} onCancel={onClose} />
      </form>
    </Modal>
  );
}
