import { useEffect, useMemo, useState } from "react";
import {
  Boxes,
  CircleAlert,
  CircleCheck,
  FlaskConical,
  Loader2,
  Pencil,
  Plus,
  Server,
  Trash,
} from "lucide-react";
import {
  EmptyNote,
  GlyphButton,
  InlineError,
  LoadingLines,
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
  capabilityLabel,
  contextWindowToNumber,
  draftHeaderRules,
  emptyModelDraft,
  emptyProviderDraft,
  formatContextWindow,
  isCapabilityId,
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
import { capabilityIcons } from "./capabilityIcons";
import { ModelForm } from "./ModelForm";
import { ProviderForm } from "./ProviderForm";
import { useAsyncAction } from "../../hooks/useAsyncAction";
import { useRegisterSelection } from "../../hooks/useRegisterSelection";
import { useLiveRevision } from "../../app/useLiveRevision";
import { connectivityLabel, connectivityState, connectivitySummary, formatLatency, formatPercent } from "../../lib/telemetry";
import {
  queryTelemetry,
  testProvider,
  type ProbeResult,
  type ProviderConnectivity,
  type TelemetrySnapshot,
} from "../../services/telemetry";

function hostLabel(baseUrl: string): string {
  try {
    return new URL(baseUrl).host;
  } catch {
    return baseUrl;
  }
}

export function ProvidersPage() {
  const [providers, setProviders] = useState<Provider[]>([]);
  const [models, setModels] = useState<UpstreamModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [modelError, setModelError] = useState<string | null>(null);
  const [modelDraft, setModelDraft] = useState<ModelDraft | null>(null);
  const [confirmingModelId, setConfirmingModelId] = useState<string | null>(null);
  const [probe, setProbe] = useState<ProbeResult[] | null>(null);
  const [probing, setProbing] = useState(false);
  const { busy, run } = useAsyncAction();
  const { revision } = useLiveRevision();
  const [telemetry, setTelemetry] = useState<TelemetrySnapshot | null>(null);

  const {
    selectedId,
    draft,
    savedDraft,
    pending,
    confirmingDelete,
    setDraft,
    setSavedDraft,
    setSelectedId,
    setPending,
    setConfirmingDelete,
    select,
    selectNew,
    requestSelect,
    requestNew,
    applyPending,
    saveAndSwitch,
  } = useRegisterSelection<Provider, ProviderDraft>({
    entities: providers,
    toDraft: providerToDraft,
    emptyDraft: emptyProviderDraft,
    isDirty: isProviderDraftDirty,
    onSelect: () => {
      setModelDraft(null);
      setFormError(null);
      setModelError(null);
      setConfirmingModelId(null);
      setProbe(null);
    },
  });

  const connectivityById = useMemo(() => {
    const map = new Map<string, ProviderConnectivity>();
    telemetry?.connectivity.forEach((item) => map.set(item.providerId, item));
    return map;
  }, [telemetry]);
  const coolingModels = useMemo(() => new Set(telemetry?.cooling ?? []), [telemetry]);

  const selectedProvider =
    selectedId && selectedId !== "new"
      ? providers.find((provider) => provider.id === selectedId) ?? null
      : null;
  const selectedModels = selectedProvider ? modelsForProvider(models, selectedProvider.id) : [];
  const autoBrand = draft
    ? detectBrand([
        draft.name,
        ...draft.endpoints.map((endpoint) => endpoint.baseUrl),
        ...selectedModels.map((model) => model.modelId),
      ])
    : null;

  useEffect(() => {
    let alive = true;
    queryTelemetry("day")
      .then((next) => {
        if (alive) setTelemetry(next);
      })
      .catch(() => {});
    return () => {
      alive = false;
    };
  }, [revision]);

  const runProbe = async () => {
    if (!selectedProvider) return;
    setProbe(null);
    setProbing(true);
    try {
      const results = await testProvider(selectedProvider.id);
      setProbe(results);
    } catch (error) {
      setProbe([
        { protocol: "", ok: false, httpStatus: null, latencyMs: 0, model: "", error: String(error) },
      ]);
    } finally {
      setProbing(false);
    }
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
    const saved = await run(
      async () => {
        const result = await saveProvider({
          id: draft.id,
          name: draft.name.trim(),
          apiKey: draft.apiKey,
          endpoints: draft.endpoints.map((endpoint) => ({
            id: endpoint.id,
            protocol: endpoint.protocol,
            baseUrl: endpoint.baseUrl.trim(),
            authScheme: endpoint.authScheme,
            enabled: endpoint.enabled,
          })),
          extraHeaders: parseExtraHeaders(draft.extraHeadersText),
          headerRules: draftHeaderRules(draft),
          icon: draft.icon,
          iconTint: draft.iconTint,
          enabled: draft.enabled,
        });
        await refreshLists();
        setSelectedId(result.id);
        setDraft(providerToDraft(result));
        setSavedDraft(providerToDraft(result));
        setModelDraft(null);
        return result;
      },
      setFormError,
    );
    return saved !== undefined;
  };

  const discardDraft = () => {
    if (!savedDraft) return;
    setDraft({ ...savedDraft });
    setFormError(null);
  };

  const setProviderAppearance = async (
    patch: { icon?: string | null; iconTint?: IconTint },
  ) => {
    if (!draft) return;
    const icon = patch.icon !== undefined ? patch.icon : draft.icon;
    const iconTint = patch.iconTint ?? draft.iconTint;
    const previousDraft = draft;
    const previousSaved = savedDraft;
    setDraft({ ...draft, icon, iconTint });
    if (!selectedProvider) return;
    setSavedDraft((prev) => (prev ? { ...prev, icon, iconTint } : prev));
    await run(
      async () => {
        await saveProvider({
          id: selectedProvider.id,
          name: selectedProvider.name,
          apiKey: selectedProvider.apiKey,
          endpoints: selectedProvider.endpoints.map((endpoint) => ({
            id: endpoint.id,
            protocol: endpoint.protocol,
            baseUrl: endpoint.baseUrl,
            authScheme: endpoint.authScheme,
            enabled: endpoint.enabled,
          })),
          extraHeaders: selectedProvider.extraHeaders,
          headerRules: selectedProvider.headerRules,
          icon,
          iconTint,
          enabled: selectedProvider.enabled,
        });
        await refreshLists();
      },
      setFormError,
      () => {
        setDraft(previousDraft);
        setSavedDraft(previousSaved);
      },
    );
  };

  const deleteSelected = async () => {
    if (!selectedProvider) return;
    await run(
      async () => {
        await deleteProvider(selectedProvider.id);
        const nextProviders = await refreshLists();
        setConfirmingDelete(false);
        if (nextProviders.length > 0) select(nextProviders[0]);
        else selectNew();
      },
      setError,
    );
  };

  const submitModel = async () => {
    if (!modelDraft) return;
    const message = validateModelDraft(modelDraft);
    if (message) {
      setModelError(message);
      return;
    }
    await run(
      async () => {
        await saveUpstreamModel({
          id: modelDraft.id,
          providerId: modelDraft.providerId,
          modelId: modelDraft.modelId.trim(),
          displayName: modelDraft.displayName.trim() || modelDraft.modelId.trim(),
          inputPrice: priceToNumber(modelDraft.inputPrice),
          outputPrice: priceToNumber(modelDraft.outputPrice),
          cacheReadPrice: priceToNumber(modelDraft.cacheReadPrice),
          cacheCreationPrice: priceToNumber(modelDraft.cacheCreationPrice),
          contextWindow: contextWindowToNumber(modelDraft.contextWindow),
          capabilities: modelDraft.capabilities,
          enabled: modelDraft.enabled,
        });
        setModelDraft(null);
        await refreshLists();
      },
      setModelError,
    );
  };

  const toggleModel = async (model: UpstreamModel) => {
    await run(
      async () => {
        await saveUpstreamModel({
          id: model.id,
          providerId: model.providerId,
          modelId: model.modelId,
          displayName: model.displayName,
          inputPrice: model.inputPrice,
          outputPrice: model.outputPrice,
          cacheReadPrice: model.cacheReadPrice,
          cacheCreationPrice: model.cacheCreationPrice,
          contextWindow: model.contextWindow,
          capabilities: model.capabilities,
          enabled: !model.enabled,
        });
        await refreshLists();
      },
      setError,
    );
  };

  const setModelAppearance = async (
    model: UpstreamModel,
    patch: { icon?: string | null; iconTint?: IconTint },
  ) => {
    await run(
      async () => {
        await saveUpstreamModel({
          id: model.id,
          providerId: model.providerId,
          modelId: model.modelId,
          displayName: model.displayName,
          inputPrice: model.inputPrice,
          outputPrice: model.outputPrice,
          cacheReadPrice: model.cacheReadPrice,
          cacheCreationPrice: model.cacheCreationPrice,
          contextWindow: model.contextWindow,
          capabilities: model.capabilities,
          icon: patch.icon !== undefined ? patch.icon : model.icon,
          iconTint: patch.iconTint ?? model.iconTint,
          enabled: model.enabled,
        });
        await refreshLists();
      },
      setError,
    );
  };

  const removeModel = async (id: string) => {
    await run(
      async () => {
        await deleteUpstreamModel(id);
        setConfirmingModelId(null);
        if (modelDraft?.id === id) setModelDraft(null);
        await refreshLists();
      },
      setError,
    );
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
                      ...provider.endpoints.map((endpoint) => endpoint.baseUrl),
                      ...providerModels.map((model) => model.modelId),
                    ]);
                    const connection = connectivityById.get(provider.id) ?? null;
                    const isCooling = providerModels.some((model) => coolingModels.has(model.id));
                    const state = connectivityState(connection, isCooling);
                    const warn = state === "error";
                    const meta = provider.enabled
                      ? `${providerModels.length} 个模型 · ${connectivitySummary(connection, isCooling)}`
                      : `已停用 · ${providerModels.length} 个模型`;
                    const metaDetail =
                      connection && connection.total > 0
                        ? `近 24 小时${connectivityLabel(state)} · 成功率 ${formatPercent(connection.successRate)} · 平均延迟 ${formatLatency(connection.avgLatencyMs)}`
                        : "近 24 小时暂无调用";
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
                          <span className="register-name" title={provider.name}>{provider.name}</span>
                          <span className={`register-meta${warn ? " is-warn" : ""}`} title={provider.enabled ? metaDetail : undefined}>
                            {meta}
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
                      onChange={(value) => void setProviderAppearance({ icon: value })}
                      onTintChange={(value) => void setProviderAppearance({ iconTint: value })}
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
                          <GlyphButton
                            label={probing ? "正在探测…" : "连通性测试"}
                            disabled={busy || probing}
                            onClick={() => void runProbe()}
                          >
                            {probing ? (
                              <Loader2 className="is-spinning" aria-hidden="true" />
                            ) : (
                              <FlaskConical aria-hidden="true" />
                            )}
                          </GlyphButton>
                          <GlyphButton label="删除该上游" danger disabled={busy} onClick={() => setConfirmingDelete(true)}>
                            <Trash aria-hidden="true" />
                          </GlyphButton>
                        </>
                      ) : null}
                    </div>
                  </div>

                  {selectedId !== "new" ? (
                    <p className="sheet-summary">
                      <span className="sheet-summary-protocols">
                        {draft.endpoints
                          .map((endpoint) => protocolLabel[endpoint.protocol])
                          .join(" · ")}
                      </span>
                      <span className="sheet-summary-sep" aria-hidden="true">·</span>
                      <span
                        className="sheet-summary-host"
                        title={draft.endpoints.map((endpoint) => endpoint.baseUrl).join("\n")}
                      >
                        {hostLabel(draft.endpoints[0]?.baseUrl ?? "")}
                      </span>
                      <span className="sheet-summary-sep" aria-hidden="true">·</span>
                      <span>{selectedModels.length} 个模型</span>
                    </p>
                  ) : null}

                  {probe ? (
                    <div className="probe-results" role="status">
                      {probe.map((result, index) => (
                        <p
                          className={`probe-result${result.ok ? " is-ok" : " is-fail"}`}
                          key={result.protocol || index}
                        >
                          {result.ok ? (
                            <CircleCheck aria-hidden="true" />
                          ) : (
                            <CircleAlert aria-hidden="true" />
                          )}
                          <span className="probe-protocol">
                            {result.protocol
                              ? protocolLabel[result.protocol as keyof typeof protocolLabel] ??
                                result.protocol
                              : "探测"}
                          </span>
                          {result.ok
                            ? `连通正常 · 延迟 ${formatLatency(result.latencyMs)}`
                            : `失败：${result.error ?? "未知错误"}`}
                        </p>
                      ))}
                    </div>
                  ) : null}

                  {pending ? (
                    <div className="pending-bar">
                      <span>有未保存的修改，切换前要保存吗？</span>
                      <button className="quiet-button is-primary" type="button" onClick={() => void saveAndSwitch(submitProvider)} disabled={busy}>
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
                            <span className="model-mark">
                              <IconPicker
                                icon={model.icon}
                                auto={detectBrand([selectedProvider.name, model.modelId])}
                                tint={model.iconTint}
                                size={20}
                                enabled={model.enabled}
                                fallback={<Boxes aria-hidden="true" />}
                                disabled={busy}
                                onChange={(value) => void setModelAppearance(model, { icon: value })}
                                onTintChange={(value) => void setModelAppearance(model, { iconTint: value })}
                              />
                            </span>
                            <div className="model-body">
                              <div className="model-line">
                                <span className="model-name">{model.displayName}</span>
                                <code className="model-id" title={model.modelId}>{model.modelId}</code>
                              </div>
                              <div className="model-specs">
                                {model.contextWindow > 0 ? (
                                  <span
                                    className="model-context"
                                    title={`上下文 ${model.contextWindow.toLocaleString("zh-CN")} tokens`}
                                  >
                                    {formatContextWindow(model.contextWindow)}
                                  </span>
                                ) : null}
                                <span className="model-price-group">
                                  <span className="price-cell">
                                    <em>入</em>
                                    <b>${model.inputPrice}</b>
                                  </span>
                                  <span className="price-cell">
                                    <em>出</em>
                                    <b>${model.outputPrice}</b>
                                  </span>
                                  <span className="price-cell" title="缓存读单价 · $ / 百万 token">
                                    <em>缓存读</em>
                                    <b>${model.cacheReadPrice}</b>
                                  </span>
                                </span>
                                {model.capabilities.filter(isCapabilityId).length > 0 ? (
                                  <span className="model-caps">
                                    {model.capabilities.filter(isCapabilityId).map((id) => {
                                      const Icon = capabilityIcons[id];
                                      return (
                                        <span className="model-cap" key={id} title={capabilityLabel[id]}>
                                          <Icon aria-hidden="true" />
                                          {capabilityLabel[id]}
                                        </span>
                                      );
                                    })}
                                  </span>
                                ) : null}
                              </div>
                            </div>
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
