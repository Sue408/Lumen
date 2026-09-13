import { useState, type ReactNode } from "react";
import { ChevronDown, Repeat } from "lucide-react";
import { InlineError, SaveBar, SectionTitle } from "../../components/ConfigControls";
import {
  headerRulesFromDraft,
  isHeaderDraftDirty,
  type ProviderHeaderDraft,
} from "./providerModel";
import { EndpointList } from "./EndpointList";
import { HeaderRulesHelp } from "./HeaderRulesHelp";
import { summarizeProviderHeaderRules } from "./providerHeaderModel.ts";
import type { Provider, ProviderEndpointInput } from "../../services/config";

/**
 * 提供商编辑面板。只有**请求头映射**保留显式保存（焦点少、改动可能需要斟酌）；
 * 协议端点静默保存，名称 / 密钥走弹窗即改即存。
 */
export function ProviderForm({
  provider,
  headerDraft,
  savedHeaderDraft,
  busy,
  headerError,
  onHeaderChange,
  onHeaderDiscard,
  onHeaderSave,
  onSaveEndpoints,
  children,
}: {
  provider: Provider | null;
  headerDraft: ProviderHeaderDraft;
  savedHeaderDraft: ProviderHeaderDraft;
  busy: boolean;
  headerError: string | null;
  onHeaderChange: (draft: ProviderHeaderDraft) => void;
  onHeaderDiscard: () => void;
  onHeaderSave: () => void;
  onSaveEndpoints: (endpoints: ProviderEndpointInput[]) => void;
  /** 模型区插槽：置于协议端点之后，让核心内容更靠前。 */
  children?: ReactNode;
}) {
  const [rulesOpen, setRulesOpen] = useState(false);
  const dirty = isHeaderDraftDirty(headerDraft, savedHeaderDraft);
  const set = (patch: Partial<ProviderHeaderDraft>) =>
    onHeaderChange({ ...headerDraft, ...patch });

  return (
    <>
      <form
        className="entry-form"
        onSubmit={(event) => {
          event.preventDefault();
          onHeaderSave();
        }}
      >
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
              {summarizeProviderHeaderRules(headerRulesFromDraft(headerDraft))}
            </span>
          </button>
          {rulesOpen ? (
            <div className="field-grid header-rules-panel">
              <label className="field field-wide">
                <span>透传（每行一个，支持 * 通配）</span>
                <textarea
                  rows={2}
                  value={headerDraft.forwardText}
                  onChange={(event) => set({ forwardText: event.target.value })}
                  placeholder={"非 x- 前缀的头也能放行，例如\nsession_id"}
                  spellCheck={false}
                />
              </label>
              <label className="field field-wide">
                <span>替换（每行 `来源 → 目标`）</span>
                <textarea
                  rows={2}
                  value={headerDraft.replaceText}
                  onChange={(event) => set({ replaceText: event.target.value })}
                  placeholder={"session_id → x-opencode-session"}
                  spellCheck={false}
                />
              </label>
              <label className="field field-wide">
                <span>添加（常量，每行 `name: value`）</span>
                <textarea
                  rows={2}
                  value={headerDraft.extraHeadersText}
                  onChange={(event) => set({ extraHeadersText: event.target.value })}
                  placeholder={"user-agent: opencode/local"}
                  spellCheck={false}
                />
              </label>
              <label className="field field-wide">
                <span>移除（每行一个，支持 * 通配）</span>
                <textarea
                  rows={2}
                  value={headerDraft.removeText}
                  onChange={(event) => set({ removeText: event.target.value })}
                  placeholder={"x-internal*"}
                  spellCheck={false}
                />
              </label>
            </div>
          ) : null}
        </section>
        {headerError ? <InlineError message={headerError} /> : null}
        <SaveBar
          dirty={dirty}
          busy={busy}
          label="保存请求头"
          onDiscard={onHeaderDiscard}
        />
      </form>

      {provider ? (
        <EndpointList
          key={provider.id}
          endpoints={provider.endpoints}
          busy={busy}
          onSave={onSaveEndpoints}
        />
      ) : null}

      {children}
    </>
  );
}
