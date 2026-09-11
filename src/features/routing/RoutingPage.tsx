import { Fragment, useEffect, useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  ListOrdered,
  Plus,
  Route,
  SlidersHorizontal,
  Trash,
} from "lucide-react";
import {
  EmptyNote,
  GlyphButton,
  InlineError,
  LoadingLines,
  SaveBar,
  SectionTitle,
  StatusDot,
  TogglePill,
} from "../../components/ConfigControls";
import { BrandGlyph } from "../brand/BrandMark";
import { resolveBrand, type BrandId } from "../brand/brand";
import { RegisterList } from "../../components/RegisterList";
import {
  deleteRoute,
  listProviders,
  listRoutes,
  listUpstreamModels,
  protocolLabel,
  saveRoute,
  type IconTint,
  type Protocol,
  type Provider,
  type RouteWithTargets,
  type UpstreamModel,
} from "../../services/config";
import {
  emptyRouteDraft,
  isRouteDraftDirty,
  makeTarget,
  moveTarget,
  routeToDraft,
  validateRouteDraft,
  type RouteDraft,
} from "./routingModel";
import { ProtocolHelp } from "./ProtocolHelp";
import { useFlipList } from "./useFlipList";

type Pending = { kind: "existing"; id: string } | { kind: "new" };

type ModelMark = { brand: BrandId | null; tint: IconTint; enabled: boolean };

function buildBrandLookup(providers: Provider[], models: UpstreamModel[]): Map<string, ModelMark> {
  const providerNames = new Map(providers.map((provider) => [provider.id, provider.name]));
  const lookup = new Map<string, ModelMark>();
  for (const model of models) {
    lookup.set(model.id, {
      brand: resolveBrand(model.icon, [providerNames.get(model.providerId), model.modelId]),
      tint: model.iconTint,
      enabled: model.enabled,
    });
  }
  return lookup;
}

function markTint(mark: ModelMark | null | undefined): IconTint {
  return mark && mark.enabled ? mark.tint : "ink";
}

function groupedModels(providers: Provider[], models: UpstreamModel[], protocol: Protocol) {
  return providers
    .filter((provider) => provider.protocol === protocol)
    .map((provider) => ({
      id: provider.id,
      name: provider.name,
      models: models.filter((model) => model.providerId === provider.id),
    }))
    .filter((group) => group.models.length > 0);
}

function RouteForm({
  draft,
  savedDraft,
  providers,
  models,
  brands,
  busy,
  error,
  onChange,
  onDiscard,
  onSubmit,
}: {
  draft: RouteDraft;
  savedDraft: RouteDraft;
  providers: Provider[];
  models: UpstreamModel[];
  brands: Map<string, ModelMark>;
  busy: boolean;
  error: string | null;
  onChange: (draft: RouteDraft) => void;
  onDiscard: () => void;
  onSubmit: () => void;
}) {
  const set = (patch: Partial<RouteDraft>) => onChange({ ...draft, ...patch });
  const setTarget = (index: number, patch: Partial<RouteDraft["targets"][number]>) =>
    set({ targets: draft.targets.map((target, itemIndex) => (itemIndex === index ? { ...target, ...patch } : target)) });
  // 协议创建后锁定；新建时切换协议会清空已选目标（先定协议，再选目标）。
  const setProtocol = (protocol: Protocol) => {
    onChange({ ...draft, protocol, targets: [] });
  };
  const groups = groupedModels(providers, models, draft.protocol);
  const firstCompatible = groups[0]?.models[0]?.id ?? "";
  const dirty = isRouteDraftDirty(draft, savedDraft);
  const { containerRef, capture } = useFlipList<HTMLOListElement>();

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
            <span>路由别名</span>
            <input value={draft.alias} onChange={(event) => set({ alias: event.target.value })} placeholder="deepseek/deepseek-v4-flash" spellCheck={false} />
          </label>
          <label className="field">
            <span>展示名</span>
            <input value={draft.displayName} onChange={(event) => set({ displayName: event.target.value })} placeholder="留空则同别名" />
          </label>
          <label className="field">
            <span className="field-label">
              协议
              <ProtocolHelp />
            </span>
            {draft.id === null ? (
              <select value={draft.protocol} onChange={(event) => setProtocol(event.target.value as Protocol)}>
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
      </section>

      <section className="sheet-section">
        <SectionTitle
          icon={<ListOrdered aria-hidden="true" />}
          action={
            <button
              className="text-action"
              type="button"
              onClick={() => {
                capture();
                set({ targets: [...draft.targets, makeTarget(firstCompatible)] });
              }}
              disabled={busy || groups.length === 0}
            >
              ＋ 添加目标
            </button>
          }
        >
          上游目标
        </SectionTitle>

        {draft.targets.length === 0 ? (
          <EmptyNote>
            {models.length === 0
              ? "还没有可用的上游模型，请先到上游提供商页登记。"
              : groups.length === 0
                ? `还没有 ${protocolLabel[draft.protocol]} 协议的上游模型，请先到上游提供商页登记。`
                : "还没有添加目标，添加后才能保存。"}
          </EmptyNote>
        ) : (
          <ol className="target-list" ref={containerRef}>
            {draft.targets.map((target, index) => (
              <Fragment key={target.uid}>
                {index > 0 ? (
                  <li className="target-connector" aria-hidden="true">
                    ↓
                  </li>
                ) : null}
                <li
                  className={target.enabled ? "target-row" : "target-row is-off"}
                  data-flip-key={target.uid}
                >
                  <span className="target-rank">{index + 1}</span>
                  <span className="target-role">
                    {index === 0 ? "首选" : `备用 ${index + 1}`}
                  </span>
                  <BrandGlyph
                    brand={brands.get(target.upstreamModelId)?.brand ?? null}
                    tint={markTint(brands.get(target.upstreamModelId))}
                    size={18}
                    fallback={<Route aria-hidden="true" />}
                  />
                  <select value={target.upstreamModelId} onChange={(event) => setTarget(index, { upstreamModelId: event.target.value })}>
                    <option value="">选择上游模型…</option>
                    {groups.map((group) => (
                      <optgroup label={group.name} key={group.id}>
                        {group.models.map((model) => (
                          <option value={model.id} key={model.id}>
                            {model.displayName} · {model.modelId}
                          </option>
                        ))}
                      </optgroup>
                    ))}
                  </select>
                  <TogglePill
                    small
                    checked={target.enabled}
                    label={target.enabled ? "停用该目标" : "启用该目标"}
                    disabled={busy}
                    onChange={(next) => setTarget(index, { enabled: next })}
                  />
                  <div className="target-move">
                    <GlyphButton label="上移" disabled={busy || index === 0} onClick={() => { capture(); set({ targets: moveTarget(draft.targets, index, -1) }); }}>
                      <ChevronUp aria-hidden="true" />
                    </GlyphButton>
                    <GlyphButton label="下移" disabled={busy || index === draft.targets.length - 1} onClick={() => { capture(); set({ targets: moveTarget(draft.targets, index, 1) }); }}>
                      <ChevronDown aria-hidden="true" />
                    </GlyphButton>
                    <GlyphButton label="移除目标" danger disabled={busy} onClick={() => { capture(); set({ targets: draft.targets.filter((_, itemIndex) => itemIndex !== index) }); }}>
                      <Trash aria-hidden="true" />
                    </GlyphButton>
                  </div>
                </li>
              </Fragment>
            ))}
          </ol>
        )}
      </section>

      {error ? <InlineError message={error} /> : null}
      <SaveBar dirty={dirty} busy={busy} label={draft.id ? "保存路由" : "创建路由"} onDiscard={onDiscard} />
    </form>
  );
}

export function RoutingPage() {
  const [routes, setRoutes] = useState<RouteWithTargets[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [models, setModels] = useState<UpstreamModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | "new" | null>(null);
  const [draft, setDraft] = useState<RouteDraft | null>(null);
  const [savedDraft, setSavedDraft] = useState<RouteDraft | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const dirty = draft !== null && savedDraft !== null && isRouteDraftDirty(draft, savedDraft);
  const brands = useMemo(() => buildBrandLookup(providers, models), [providers, models]);
  const modelProtocol = useMemo(() => {
    const providerProtocol = new Map(providers.map((provider) => [provider.id, provider.protocol]));
    return new Map(models.map((model) => [model.id, providerProtocol.get(model.providerId)]));
  }, [providers, models]);

  const select = (route: RouteWithTargets) => {
    const next = routeToDraft(route);
    setSelectedId(route.id);
    setDraft(next);
    setSavedDraft(structuredClone(next));
    setFormError(null);
    setConfirmingDelete(false);
    setPending(null);
  };

  const selectNew = () => {
    setSelectedId("new");
    setDraft(emptyRouteDraft());
    setSavedDraft(emptyRouteDraft());
    setFormError(null);
    setConfirmingDelete(false);
    setPending(null);
  };

  const requestSelect = (route: RouteWithTargets) => {
    if (dirty) setPending({ kind: "existing", id: route.id });
    else select(route);
  };

  const requestNew = () => {
    if (dirty) setPending({ kind: "new" });
    else selectNew();
  };

  const refreshLists = async () => {
    const [nextRoutes, nextProviders, nextModels] = await Promise.all([
      listRoutes(),
      listProviders(),
      listUpstreamModels(),
    ]);
    setRoutes(nextRoutes);
    setProviders(nextProviders);
    setModels(nextModels);
    return nextRoutes;
  };

  useEffect(() => {
    let alive = true;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const [nextRoutes, nextProviders, nextModels] = await Promise.all([
          listRoutes(),
          listProviders(),
          listUpstreamModels(),
        ]);
        if (!alive) return;
        setRoutes(nextRoutes);
        setProviders(nextProviders);
        setModels(nextModels);
        if (nextRoutes.length > 0) select(nextRoutes[0]);
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

  const submit = async (): Promise<boolean> => {
    if (!draft) return false;
    const message = validateRouteDraft(draft, (id) => modelProtocol.get(id));
    if (message) {
      setFormError(message);
      return false;
    }
    setBusy(true);
    setFormError(null);
    try {
      const saved = await saveRoute({
        id: draft.id,
        alias: draft.alias.trim(),
        displayName: draft.displayName.trim() || draft.alias.trim(),
        protocol: draft.protocol,
        enabled: draft.enabled,
        targets: draft.targets.map((target, index) => ({
          upstreamModelId: target.upstreamModelId,
          priority: index,
          enabled: target.enabled,
        })),
      });
      await refreshLists();
      const nextDraft = routeToDraft(saved);
      setSelectedId(saved.id);
      setDraft(nextDraft);
      setSavedDraft(structuredClone(nextDraft));
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
    setDraft({ ...savedDraft, targets: savedDraft.targets.map((target) => ({ ...target })) });
    setFormError(null);
  };

  const applyPending = () => {
    if (!pending) return;
    if (pending.kind === "new") selectNew();
    else {
      const route = routes.find((item) => item.id === pending.id);
      if (route) select(route);
      else setPending(null);
    }
  };

  const saveAndSwitch = async () => {
    const ok = await submit();
    if (ok) applyPending();
  };

  const deleteSelected = async () => {
    if (!selectedId || selectedId === "new") return;
    setBusy(true);
    setError(null);
    try {
      await deleteRoute(selectedId);
      const nextRoutes = await refreshLists();
      setConfirmingDelete(false);
      if (nextRoutes.length > 0) select(nextRoutes[0]);
      else selectNew();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="routing-page">
      <div className="config-column">
        <header className="page-header">
          <div>
            <div className="eyebrow">ROUTING RECIPES</div>
            <h1>模型路由</h1>
          </div>
        </header>

        {error ? <InlineError message={error} /> : null}

        {loading ? (
          <LoadingLines rows={5} />
        ) : (
          <div className="config-workbench">
            <section className="register-pane">
              <div className="register-head">
                <span className="register-title">路由 · {routes.length} 条</span>
                <GlyphButton label="新建路由" onClick={requestNew}>
                  <Plus aria-hidden="true" />
                </GlyphButton>
              </div>

              {routes.length === 0 ? (
                <div className="register-empty">
                  <EmptyNote>还没有配置路由别名。</EmptyNote>
                  <button className="text-action" type="button" onClick={requestNew}>
                    创建第一个路由
                  </button>
                </div>
              ) : (
                <RegisterList
                  ariaLabel="路由别名列表"
                  selectedKey={selectedId === "new" ? null : selectedId}
                >
                  {routes.map((route) => {
                    const primary = [...route.targets].sort((a, b) => a.priority - b.priority)[0];
                    const mark = primary ? brands.get(primary.upstreamModelId) ?? null : null;
                    return (
                      <button
                        className={`register-select${selectedId === route.id ? " is-selected" : ""}${route.enabled ? "" : " is-off"}`}
                        type="button"
                        key={route.id}
                        data-register-key={route.id}
                        aria-current={selectedId === route.id ? "true" : undefined}
                        onClick={() => requestSelect(route)}
                      >
                        <BrandGlyph brand={mark?.brand ?? null} tint={markTint(mark)} size={20} fallback={<Route aria-hidden="true" />} />
                        <span className="register-body">
                          <span className="register-name" title={route.alias}>{route.alias}</span>
                          <span className="register-meta">
                            <span className="protocol-tag">{protocolLabel[route.protocol]}</span>
                            {route.enabled
                              ? `${route.targets.length} 条目标`
                              : `已停用 · ${route.targets.length} 条目标`}
                          </span>
                        </span>
                        <StatusDot alive={route.enabled} />
                      </button>
                    );
                  })}
                </RegisterList>
              )}
            </section>

            <section className="workbench-sheet" aria-label="路由编辑">
              {draft ? (
                <>
                  <div className="sheet-head">
                    <BrandGlyph
                      brand={draft.targets[0] ? brands.get(draft.targets[0].upstreamModelId)?.brand ?? null : null}
                      tint={draft.targets[0] ? markTint(brands.get(draft.targets[0].upstreamModelId)) : "ink"}
                      size={22}
                      className={draft.enabled ? undefined : "is-off"}
                      fallback={<Route aria-hidden="true" />}
                    />
                    <h2 className="sheet-title">{selectedId === "new" ? "新建路由" : draft.alias || "未命名路由"}</h2>
                    <TogglePill
                      checked={draft.enabled}
                      label="启用该路由"
                      disabled={busy}
                      onChange={(next) => setDraft({ ...draft, enabled: next })}
                    />
                    <div className="sheet-actions">
                      {selectedId !== "new" ? (
                        <GlyphButton label="删除该路由" danger disabled={busy} onClick={() => setConfirmingDelete(true)}>
                          <Trash aria-hidden="true" />
                        </GlyphButton>
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
                      <span>确认移除该路由别名？</span>
                      <button className="quiet-button is-danger" type="button" onClick={() => void deleteSelected()} disabled={busy}>
                        移除
                      </button>
                      <button className="quiet-button" type="button" onClick={() => setConfirmingDelete(false)} disabled={busy}>
                        取消
                      </button>
                    </div>
                  ) : null}

                  <RouteForm
                    key={selectedId ?? "new"}
                    draft={draft}
                    savedDraft={savedDraft ?? draft}
                    providers={providers}
                    models={models}
                    brands={brands}
                    busy={busy}
                    error={formError}
                    onChange={setDraft}
                    onDiscard={discardDraft}
                    onSubmit={() => void submit()}
                  />
                </>
              ) : null}
            </section>
          </div>
        )}
      </div>
    </main>
  );
}
