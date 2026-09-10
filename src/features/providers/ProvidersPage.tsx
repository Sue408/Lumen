import { useEffect, useState } from "react";
import {
  Boxes,
  FlaskConical,
  Pencil,
  Plus,
  Server,
  SlidersHorizontal,
  Trash,
} from "lucide-react";
import {
  EmptyNote,
  FormActions,
  GlyphButton,
  InlineError,
  LoadingLines,
  SaveBar,
  SectionTitle,
  StatusDot,
  TogglePill,
} from "../../components/ConfigControls";
import { Modal } from "../../components/Modal";
import { RegisterList } from "../../components/RegisterList";
import { BrandGlyph } from "../brand/BrandMark";
import { IconPicker } from "../brand/IconPicker";
import { detectBrand, resolveBrand } from "../brand/brand";
import {
  deleteProvider,
  deleteUpstreamModel,
  listProviders,
  listUpstreamModels,
  saveProvider,
  saveUpstreamModel,
  type IconTint,
  type Provider,
  type UpstreamModel,
} from "../../services/config";
import {
  authSchemeLabel,
  emptyModelDraft,
  emptyProviderDraft,
  isProviderDraftDirty,
  modelToDraft,
  modelsForProvider,
  parseExtraHeaders,
  priceToNumber,
  protocolLabel,
  providerToDraft,
  validateModelDraft,
  validateProviderDraft,
  type ModelDraft,
  type ProviderDraft,
} from "./providerModel";

type Pending = { kind: "existing"; id: string } | { kind: "new" };

function ProviderForm({
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
            </select>
          </label>
          <label className="field">
            <span>协议</span>
            <select value={draft.protocol} onChange={(event) => set({ protocol: event.target.value as ProviderDraft["protocol"] })}>
              <option value="openai">{protocolLabel.openai}</option>
              <option value="anthropic">{protocolLabel.anthropic}</option>
            </select>
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

function ModelForm({
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
          <span>输入单价 · 元 / 百万</span>
          <input inputMode="decimal" value={draft.inputPrice} onChange={(event) => set({ inputPrice: event.target.value })} />
        </label>
        <label className="field">
          <span>输出单价 · 元 / 百万</span>
          <input inputMode="decimal" value={draft.outputPrice} onChange={(event) => set({ outputPrice: event.target.value })} />
        </label>
        <div className="field-inline">
          <TogglePill checked={draft.enabled} label="启用该模型" disabled={busy} onChange={(next) => set({ enabled: next })} />
        </div>
      </div>
      {error ? <InlineError message={error} /> : null}
      <FormActions busy={busy} submitLabel={draft.id ? "保存模型" : "登记模型"} onCancel={onCancel} />
    </form>
  );
}

export function ProvidersPage() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [models, setModels] = useState<UpstreamModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [modelError, setModelError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | "new" | null>(null);
  const [draft, setDraft] = useState<ProviderDraft | null>(null);
  const [savedDraft, setSavedDraft] = useState<ProviderDraft | null>(null);
  const [modelDraft, setModelDraft] = useState<ModelDraft | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [confirmingModelId, setConfirmingModelId] = useState<string | null>(null);

  const selectedProvider =
    selectedId && selectedId !== "new"
      ? providers.find((provider) => provider.id === selectedId) ?? null
      : null;
  const selectedModels = selectedProvider ? modelsForProvider(models, selectedProvider.id) : [];
  const autoBrand = draft
    ? detectBrand([draft.name, draft.baseUrl, ...selectedModels.map((model) => model.modelId)])
    : null;
  const dirty = draft !== null && savedDraft !== null && isProviderDraftDirty(draft, savedDraft);

  const select = (provider: Provider) => {
    setSelectedId(provider.id);
    setDraft(providerToDraft(provider));
    setSavedDraft(providerToDraft(provider));
    setModelDraft(null);
    setFormError(null);
    setModelError(null);
    setConfirmingDelete(false);
    setConfirmingModelId(null);
    setPending(null);
  };

  const selectNew = () => {
    setSelectedId("new");
    setDraft(emptyProviderDraft());
    setSavedDraft(emptyProviderDraft());
    setModelDraft(null);
    setFormError(null);
    setModelError(null);
    setConfirmingDelete(false);
    setConfirmingModelId(null);
    setPending(null);
  };

  const requestSelect = (provider: Provider) => {
    if (dirty) setPending({ kind: "existing", id: provider.id });
    else select(provider);
  };

  const requestNew = () => {
    if (dirty) setPending({ kind: "new" });
    else selectNew();
  };

  const refreshLists = async () => {
    const [nextProviders, nextModels] = await Promise.all([
      listProviders(),
      listUpstreamModels(),
    ]);
    setProviders(nextProviders);
    setModels(nextModels);
    return nextProviders;
  };

  useEffect(() => {
    let alive = true;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const [nextProviders, nextModels] = await Promise.all([
          listProviders(),
          listUpstreamModels(),
        ]);
        if (!alive) return;
        setProviders(nextProviders);
        setModels(nextModels);
        if (nextProviders.length > 0) select(nextProviders[0]);
        else selectNew();
      } catch (err) {
        if (alive) setError(String(err));
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  const submitProvider = async (): Promise<boolean> => {
    if (!draft) return false;
    const message = validateProviderDraft(draft);
    if (message) {
      setFormError(message);
      return false;
    }
    setBusy(true);
    setFormError(null);
    try {
      const saved = await saveProvider({
        id: draft.id,
        name: draft.name.trim(),
        baseUrl: draft.baseUrl.trim(),
        apiKey: draft.apiKey,
        authScheme: draft.authScheme,
        protocol: draft.protocol,
        extraHeaders: parseExtraHeaders(draft.extraHeadersText),
        icon: draft.icon,
        iconTint: draft.iconTint,
        enabled: draft.enabled,
      });
      await refreshLists();
      setSelectedId(saved.id);
      setDraft(providerToDraft(saved));
      setSavedDraft(providerToDraft(saved));
      setModelDraft(null);
      return true;
    } catch (err) {
      setFormError(String(err));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const discardDraft = () => {
    if (!savedDraft) return;
    setDraft({ ...savedDraft });
    setFormError(null);
  };

  const applyPending = () => {
    if (!pending) return;
    if (pending.kind === "new") selectNew();
    else {
      const provider = providers.find((item) => item.id === pending.id);
      if (provider) select(provider);
      else setPending(null);
    }
  };

  const saveAndSwitch = async () => {
    const ok = await submitProvider();
    if (ok) applyPending();
  };

  const deleteSelected = async () => {
    if (!selectedProvider) return;
    setBusy(true);
    setError(null);
    try {
      await deleteProvider(selectedProvider.id);
      const nextProviders = await refreshLists();
      setConfirmingDelete(false);
      if (nextProviders.length > 0) select(nextProviders[0]);
      else selectNew();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const submitModel = async () => {
    if (!modelDraft) return;
    const message = validateModelDraft(modelDraft);
    if (message) {
      setModelError(message);
      return;
    }
    setBusy(true);
    setModelError(null);
    try {
      await saveUpstreamModel({
        id: modelDraft.id,
        providerId: modelDraft.providerId,
        modelId: modelDraft.modelId.trim(),
        displayName: modelDraft.displayName.trim() || modelDraft.modelId.trim(),
        inputPrice: priceToNumber(modelDraft.inputPrice),
        outputPrice: priceToNumber(modelDraft.outputPrice),
        enabled: modelDraft.enabled,
      });
      setModelDraft(null);
      await refreshLists();
    } catch (err) {
      setModelError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const toggleModel = async (model: UpstreamModel) => {
    setBusy(true);
    setError(null);
    try {
      await saveUpstreamModel({
        id: model.id,
        providerId: model.providerId,
        modelId: model.modelId,
        displayName: model.displayName,
        inputPrice: model.inputPrice,
        outputPrice: model.outputPrice,
        enabled: !model.enabled,
      });
      await refreshLists();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const setModelAppearance = async (
    model: UpstreamModel,
    patch: { icon?: string | null; iconTint?: IconTint },
  ) => {
    setBusy(true);
    setError(null);
    try {
      await saveUpstreamModel({
        id: model.id,
        providerId: model.providerId,
        modelId: model.modelId,
        displayName: model.displayName,
        inputPrice: model.inputPrice,
        outputPrice: model.outputPrice,
        icon: patch.icon !== undefined ? patch.icon : model.icon,
        iconTint: patch.iconTint ?? model.iconTint,
        enabled: model.enabled,
      });
      await refreshLists();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const removeModel = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await deleteUpstreamModel(id);
      setConfirmingModelId(null);
      if (modelDraft?.id === id) setModelDraft(null);
      await refreshLists();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="providers-page">
      <div className="config-column">
        <header className="page-header">
          <div>
            <div className="eyebrow">UPSTREAM REGISTER</div>
            <h1>上游提供商</h1>
          </div>
        </header>

        {error ? <InlineError message={error} /> : null}

        {loading ? (
          <LoadingLines rows={5} />
        ) : (
          <div className="config-workbench">
            <section className="register-pane">
              <div className="register-head">
                <span className="register-title">上游 · {providers.length} 家</span>
                <GlyphButton label="登记上游" onClick={requestNew}>
                  <Plus aria-hidden="true" />
                </GlyphButton>
              </div>

              {providers.length === 0 ? (
                <div className="register-empty">
                  <EmptyNote>还没有登记上游提供商。</EmptyNote>
                  <button className="text-action" type="button" onClick={requestNew}>
                    登记第一个上游
                  </button>
                </div>
              ) : (
                <RegisterList
                  ariaLabel="上游提供商列表"
                  selectedKey={selectedId === "new" ? null : selectedId}
                >
                  {providers.map((provider) => {
                    const providerModels = modelsForProvider(models, provider.id);
                    const brand = resolveBrand(provider.icon, [
                      provider.name,
                      provider.baseUrl,
                      ...providerModels.map((model) => model.modelId),
                    ]);
                    return (
                      <button
                        className={`register-select${selectedId === provider.id ? " is-selected" : ""}${provider.enabled ? "" : " is-off"}`}
                        type="button"
                        key={provider.id}
                        data-register-key={provider.id}
                        aria-current={selectedId === provider.id ? "true" : undefined}
                        onClick={() => requestSelect(provider)}
                      >
                        <BrandGlyph brand={brand} size={20} tint={provider.enabled ? provider.iconTint : "ink"} fallback={<Server aria-hidden="true" />} />
                        <span className="register-body">
                          <span className="register-name">{provider.name}</span>
                          <span className="register-meta">
                            {provider.enabled ? `${providerModels.length} 个模型` : `已停用 · ${providerModels.length} 个模型`}
                          </span>
                        </span>
                        <StatusDot alive={provider.enabled} />
                      </button>
                    );
                  })}
                </RegisterList>
              )}
            </section>

            <section className="workbench-sheet" aria-label="上游提供商编辑">
              {draft ? (
                <>
                  <div className="sheet-head">
                    <IconPicker
                      icon={draft.icon}
                      auto={autoBrand}
                      tint={draft.iconTint}
                      size={22}
                      enabled={draft.enabled}
                      glyphClassName={draft.enabled ? undefined : "is-off"}
                      fallback={<Server aria-hidden="true" />}
                      disabled={busy}
                      onChange={(value) => setDraft({ ...draft, icon: value })}
                      onTintChange={(value) => setDraft({ ...draft, iconTint: value })}
                    />
                    <h2 className="sheet-title">{selectedId === "new" ? "新上游" : draft.name || "未命名上游"}</h2>
                    <TogglePill
                      checked={draft.enabled}
                      label="启用该上游"
                      disabled={busy}
                      onChange={(next) => setDraft({ ...draft, enabled: next })}
                    />
                    <div className="sheet-actions">
                      {selectedId !== "new" ? (
                        <>
                          <GlyphButton label="连通性测试尚未接入" disabled onClick={() => {}}>
                            <FlaskConical aria-hidden="true" />
                          </GlyphButton>
                          <GlyphButton label="删除该上游" danger disabled={busy} onClick={() => setConfirmingDelete(true)}>
                            <Trash aria-hidden="true" />
                          </GlyphButton>
                        </>
                      ) : null}
                    </div>
                  </div>

                  {pending ? (
                    <div className="pending-bar">
                      <span>有未保存的修改，切换前要保存吗？</span>
                      <button className="quiet-button is-primary" type="button" onClick={() => void saveAndSwitch()} disabled={busy}>
                        保存并切换
                      </button>
                      <button className="quiet-button" type="button" onClick={applyPending} disabled={busy}>
                        放弃并切换
                      </button>
                      <button className="quiet-button" type="button" onClick={() => setPending(null)} disabled={busy}>
                        取消
                      </button>
                    </div>
                  ) : null}

                  {confirmingDelete ? (
                    <div className="confirm-bar">
                      <span>
                        将一并移除其 {selectedModels.length} 个上游模型，引用这些模型的路由目标也会被清除。
                      </span>
                      <button className="quiet-button is-danger" type="button" onClick={() => void deleteSelected()} disabled={busy}>
                        移除
                      </button>
                      <button className="quiet-button" type="button" onClick={() => setConfirmingDelete(false)} disabled={busy}>
                        取消
                      </button>
                    </div>
                  ) : null}

                  <ProviderForm
                    key={selectedId ?? "new"}
                    draft={draft}
                    savedDraft={savedDraft ?? draft}
                    busy={busy}
                    error={formError}
                    onChange={setDraft}
                    onDiscard={discardDraft}
                    onSubmit={() => void submitProvider()}
                  />

                  {selectedProvider ? (
                    <section className="sheet-section">
                      <SectionTitle
                        icon={<Boxes aria-hidden="true" />}
                        action={
                          <button
                            className="text-action"
                            type="button"
                            onClick={() => {
                              setModelError(null);
                              setModelDraft(emptyModelDraft(selectedProvider.id));
                            }}
                            disabled={busy}
                          >
                            ＋ 登记模型
                          </button>
                        }
                      >
                        上游模型
                      </SectionTitle>

                      <ul className="model-list">
                        {selectedModels.map((model) => (
                          <li className={model.enabled ? "model-row" : "model-row is-off"} key={model.id}>
                            <IconPicker
                              icon={model.icon}
                              auto={detectBrand([selectedProvider.name, model.modelId])}
                              tint={model.iconTint}
                              size={18}
                              enabled={model.enabled}
                              fallback={<Boxes aria-hidden="true" />}
                              disabled={busy}
                              onChange={(value) => void setModelAppearance(model, { icon: value })}
                              onTintChange={(value) => void setModelAppearance(model, { iconTint: value })}
                            />
                            <span className="model-name">{model.displayName}</span>
                            <code className="model-id">{model.modelId}</code>
                            <span className="model-price">入 ¥{model.inputPrice} / 出 ¥{model.outputPrice}</span>
                            <div className="model-row-actions">
                              <TogglePill
                                small
                                checked={model.enabled}
                                label={model.enabled ? "停用该模型" : "启用该模型"}
                                disabled={busy}
                                onChange={() => void toggleModel(model)}
                              />
                              <GlyphButton
                                label="编辑模型"
                                disabled={busy}
                                onClick={() => {
                                  setModelError(null);
                                  setModelDraft(modelToDraft(model));
                                }}
                              >
                                <Pencil aria-hidden="true" />
                              </GlyphButton>
                              <GlyphButton
                                label="删除模型"
                                danger
                                disabled={busy}
                                onClick={() => setConfirmingModelId(model.id)}
                              >
                                <Trash aria-hidden="true" />
                              </GlyphButton>
                            </div>
                          </li>
                        ))}
                        {selectedModels.length === 0 ? (
                          <li className="model-row is-empty">
                            <EmptyNote>还没有登记模型。</EmptyNote>
                          </li>
                        ) : null}
                      </ul>

                      {confirmingModelId ? (
                        <div className="confirm-bar">
                          <span>确认移除该模型？引用它的路由目标也会一并清除。</span>
                          <button className="quiet-button is-danger" type="button" onClick={() => void removeModel(confirmingModelId)} disabled={busy}>
                            移除
                          </button>
                          <button className="quiet-button" type="button" onClick={() => setConfirmingModelId(null)} disabled={busy}>
                            取消
                          </button>
                        </div>
                      ) : null}
                    </section>
                  ) : null}
                </>
              ) : null}
            </section>
          </div>
        )}
      </div>

      {modelDraft ? (
        <Modal
          title={modelDraft.id ? "编辑上游模型" : "登记上游模型"}
          onClose={() => setModelDraft(null)}
        >
          <ModelForm
            draft={modelDraft}
            busy={busy}
            error={modelError}
            onChange={setModelDraft}
            onSubmit={submitModel}
            onCancel={() => setModelDraft(null)}
          />
        </Modal>
      ) : null}
    </main>
  );
}
