import { useEffect, useMemo, useState } from "react";
import {
  ChevronDown,
  ChevronUp,
  ListOrdered,
  Pencil,
  Plus,
  Route,
  Trash,
} from "lucide-react";
import {
  EmptyNote,
  GlyphButton,
  InlineError,
  InlineWarning,
  LoadingLines,
  SectionTitle,
  StatusDot,
  TogglePill,
} from "../../components/ConfigControls";
import { BrandGlyph } from "../brand/BrandMark";
import { IconPicker } from "../brand/IconPicker";
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
  type RouteTargetInput,
  type RouteWithTargets,
  type UpstreamModel,
} from "../../services/config";
import { hasUsableBackup, moveTarget } from "./routingModel";
import {
  buildBrandLookup,
  markTint,
  primaryModelMark,
  routeAppearance,
} from "./routingAppearance";
import { ModelPicker } from "./ModelPicker";
import { ProtocolHelp } from "./ProtocolHelp";
import { RouteBasicsModal, type RouteBasicsDraft } from "./RouteBasicsModal";
import { useAsyncAction } from "../../hooks/useAsyncAction";

function groupedModels(providers: Provider[], models: UpstreamModel[], protocol: Protocol) {
  return providers
    .filter((provider) =>
      provider.endpoints.some(
        (endpoint) => endpoint.protocol === protocol && endpoint.enabled,
      ),
    )
    .map((provider) => ({
      id: provider.id,
      name: provider.name,
      models: models.filter((model) => model.providerId === provider.id),
    }))
    .filter((group) => group.models.length > 0);
}

function sortedTargets(route: RouteWithTargets) {
  return [...route.targets].sort((a, b) => a.priority - b.priority);
}

function targetInputs(targets: RouteWithTargets["targets"]): RouteTargetInput[] {
  return targets.map((target, index) => ({
    upstreamModelId: target.upstreamModelId,
    priority: index,
    enabled: target.enabled,
  }));
}

type RoutePatch = {
  alias?: string;
  displayName?: string;
  icon?: string | null;
  iconTint?: IconTint | null;
  enabled?: boolean;
  targets?: RouteTargetInput[];
};

export function RoutingPage() {
  const [routes, setRoutes] = useState<RouteWithTargets[]>([]);
  const [providers, setProviders] = useState<Provider[]>([]);
  const [models, setModels] = useState<UpstreamModel[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [actionError, setActionError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);
  const [basicsDraft, setBasicsDraft] = useState<RouteBasicsDraft | null>(null);
  const [basicsError, setBasicsError] = useState<string | null>(null);
  const { busy, run } = useAsyncAction();

  const brands = useMemo(() => buildBrandLookup(providers, models), [providers, models]);
  const modelById = useMemo(() => new Map(models.map((model) => [model.id, model])), [models]);
  const providerNameById = useMemo(
    () => new Map(providers.map((provider) => [provider.id, provider.name])),
    [providers],
  );
  const selected = routes.find((route) => route.id === selectedId) ?? null;
  const targets = selected ? sortedTargets(selected) : [];
  const appearance = selected
    ? routeAppearance(selected, primaryModelMark(selected.targets, brands))
    : null;
  const groups = selected ? groupedModels(providers, models, selected.protocol) : [];
  const taken = new Set(targets.map((target) => target.upstreamModelId));
  const hasEnabledTarget = targets.some((target) => target.enabled);
  const enabledCount = targets.filter((target) => target.enabled).length;

  const refreshRoutes = async () => {
    const next = await listRoutes();
    setRoutes(next);
    return next;
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
        setSelectedId(nextRoutes[0]?.id ?? null);
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

  // 所有改动即改即存：从已保存实体出发，只覆盖 patch 里的字段。
  const persist = (route: RouteWithTargets, patch: RoutePatch) =>
    saveRoute({
      id: route.id,
      alias: patch.alias ?? route.alias,
      displayName: patch.displayName ?? route.displayName,
      protocol: route.protocol,
      icon: patch.icon !== undefined ? patch.icon : route.icon,
      iconTint: patch.iconTint !== undefined ? patch.iconTint : route.iconTint,
      enabled: patch.enabled !== undefined ? patch.enabled : route.enabled,
      targets: patch.targets ?? targetInputs(sortedTargets(route)),
    });

  const apply = (route: RouteWithTargets, patch: RoutePatch) =>
    run(async () => {
      await persist(route, patch);
      await refreshRoutes();
    }, setActionError);

  const openBasicsNew = () => {
    setBasicsError(null);
    setBasicsDraft({ id: null, alias: "", displayName: "", protocol: "openai" });
  };

  const openBasicsEdit = (route: RouteWithTargets) => {
    setBasicsError(null);
    setBasicsDraft({
      id: route.id,
      alias: route.alias,
      displayName: route.displayName,
      protocol: route.protocol,
    });
  };

  const submitBasics = async (draft: RouteBasicsDraft) => {
    const alias = draft.alias.trim();
    if (!alias) {
      setBasicsError("请填写路由别名。");
      return;
    }
    if (/\s/.test(alias)) {
      setBasicsError("别名不能包含空格。");
      return;
    }
    const displayName = draft.displayName.trim() || alias;
    const ok = await run(async () => {
      if (draft.id === null) {
        const created = await saveRoute({
          id: null,
          alias,
          displayName,
          protocol: draft.protocol,
          icon: null,
          iconTint: null,
          enabled: false,
          targets: [],
        });
        await refreshRoutes();
        setSelectedId(created.id);
      } else {
        const current = routes.find((route) => route.id === draft.id);
        if (current) await persist(current, { alias, displayName });
        await refreshRoutes();
      }
      return true;
    }, setBasicsError);
    if (ok) setBasicsDraft(null);
  };

  const toggleRoute = (route: RouteWithTargets) =>
    apply(route, { enabled: !route.enabled });

  const toggleTarget = (route: RouteWithTargets, modelId: string) =>
    apply(route, {
      targets: sortedTargets(route).map((target, index) => ({
        upstreamModelId: target.upstreamModelId,
        priority: index,
        enabled: target.upstreamModelId === modelId ? !target.enabled : target.enabled,
      })),
    });

  const moveTargetBy = (route: RouteWithTargets, index: number, delta: number) =>
    apply(route, { targets: targetInputs(moveTarget(sortedTargets(route), index, delta)) });

  const removeTarget = (route: RouteWithTargets, modelId: string) =>
    apply(route, {
      targets: targetInputs(
        sortedTargets(route).filter((target) => target.upstreamModelId !== modelId),
      ),
    });

  const addTarget = (route: RouteWithTargets, modelId: string) =>
    apply(route, {
      targets: [
        ...targetInputs(sortedTargets(route)),
        {
          upstreamModelId: modelId,
          priority: sortedTargets(route).length,
          enabled: true,
        },
      ],
    });

  const deleteSelected = async () => {
    if (!selected) return;
    await run(async () => {
      await deleteRoute(selected.id);
      const next = await refreshRoutes();
      setConfirmingDelete(false);
      setSelectedId(next[0]?.id ?? null);
    }, setError);
  };

  const selectRoute = (id: string) => {
    setSelectedId(id);
    setActionError(null);
    setConfirmingDelete(false);
  };

  return (
    <main className="routing-page">
      <div className="config-column">
        <header className="page-header">
          <div>
            <div className="eyebrow">ROUTING RECIPES</div>
            <h1>模型路由</h1>
          </div>
          <ProtocolHelp />
        </header>

        {error ? <InlineError message={error} /> : null}

        {loading ? (
          <LoadingLines rows={5} />
        ) : (
          <div className="config-workbench">
            <section className="register-pane">
              <div className="register-head">
                <span className="register-title">路由 · {routes.length} 条</span>
                <GlyphButton label="新建路由" onClick={openBasicsNew}>
                  <Plus aria-hidden="true" />
                </GlyphButton>
              </div>

              {routes.length === 0 ? (
                <div className="register-empty">
                  <EmptyNote>还没有配置路由别名。</EmptyNote>
                  <button className="text-action" type="button" onClick={openBasicsNew}>
                    创建第一个路由
                  </button>
                </div>
              ) : (
                <RegisterList ariaLabel="路由别名列表" selectedKey={selectedId}>
                  {routes.map((route) => {
                    const look = routeAppearance(route, primaryModelMark(route.targets, brands));
                    return (
                      <button
                        className={`register-select${selectedId === route.id ? " is-selected" : ""}${route.enabled ? "" : " is-off"}`}
                        type="button"
                        key={route.id}
                        data-register-key={route.id}
                        aria-current={selectedId === route.id ? "true" : undefined}
                        onClick={() => selectRoute(route.id)}
                      >
                        <BrandGlyph brand={look.brand} tint={look.tint} size={20} fallback={<Route aria-hidden="true" />} />
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

            <section className="workbench-sheet" aria-label="路由详情">
              {selected && appearance ? (
                <>
                  <div className="sheet-head">
                    <IconPicker
                      icon={selected.icon}
                      auto={appearance.auto}
                      tint={appearance.tint}
                      size={22}
                      enabled={selected.enabled}
                      glyphClassName={selected.enabled ? undefined : "is-off"}
                      fallback={<Route aria-hidden="true" />}
                      disabled={busy}
                      onChange={(value) => void apply(selected, { icon: value })}
                      onTintChange={(value) => void apply(selected, { iconTint: value })}
                    />
                    <div className="sheet-title-block">
                      <h2 className="sheet-title">{selected.displayName || selected.alias}</h2>
                      <span className="protocol-tag">{protocolLabel[selected.protocol]}</span>
                    </div>
                    <TogglePill
                      checked={selected.enabled}
                      label="启用该路由"
                      disabled={busy || (!selected.enabled && !hasEnabledTarget)}
                      onChange={() => void toggleRoute(selected)}
                    />
                    <div className="sheet-actions">
                      <GlyphButton label="编辑基础信息" disabled={busy} onClick={() => openBasicsEdit(selected)}>
                        <Pencil aria-hidden="true" />
                      </GlyphButton>
                      <GlyphButton label="删除该路由" danger disabled={busy} onClick={() => setConfirmingDelete(true)}>
                        <Trash aria-hidden="true" />
                      </GlyphButton>
                    </div>
                  </div>

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

                  <section className="sheet-section">
                    <SectionTitle
                      icon={<ListOrdered aria-hidden="true" />}
                      action={
                        <ModelPicker
                          groups={groups}
                          brands={brands}
                          taken={taken}
                          disabled={busy}
                          onSelect={(modelId) => void addTarget(selected, modelId)}
                        />
                      }
                    >
                      上游目标
                    </SectionTitle>

                    {targets.length === 0 ? (
                      <EmptyNote>
                        {models.length === 0
                          ? "还没有可用的上游模型，请先到上游提供商页登记。"
                          : groups.length === 0
                            ? `还没有 ${protocolLabel[selected.protocol]} 协议的上游模型，请先到上游提供商页登记。`
                            : "还没有添加目标。添加至少一个目标后才能启用。"}
                      </EmptyNote>
                    ) : (
                      <ol className="target-list">
                        {targets.map((target, index) => {
                          const model = modelById.get(target.upstreamModelId) ?? null;
                          const mark = brands.get(target.upstreamModelId) ?? null;
                          return (
                            <li
                              key={target.id}
                              className={target.enabled ? "target-row" : "target-row is-off"}
                            >
                              <span className="target-rank">{index + 1}</span>
                              <BrandGlyph
                                brand={mark?.brand ?? null}
                                tint={markTint(mark)}
                                size={18}
                                fallback={<Route aria-hidden="true" />}
                              />
                              <span className="target-model">
                                <span className="target-model-name">
                                  {model?.displayName ?? target.upstreamModelId}
                                </span>
                                {model ? <code className="target-model-id">{model.modelId}</code> : null}
                                {model ? (
                                  <span className="target-provider">
                                    {providerNameById.get(model.providerId) ?? ""}
                                  </span>
                                ) : null}
                              </span>
                              <TogglePill
                                small
                                checked={target.enabled}
                                label={target.enabled ? "停用该目标" : "启用该目标"}
                                disabled={busy || (target.enabled && selected.enabled && enabledCount === 1)}
                                onChange={() => void toggleTarget(selected, target.upstreamModelId)}
                              />
                              <div className="target-move">
                                <GlyphButton
                                  label="上移"
                                  disabled={busy || index === 0}
                                  onClick={() => void moveTargetBy(selected, index, -1)}
                                >
                                  <ChevronUp aria-hidden="true" />
                                </GlyphButton>
                                <GlyphButton
                                  label="下移"
                                  disabled={busy || index === targets.length - 1}
                                  onClick={() => void moveTargetBy(selected, index, 1)}
                                >
                                  <ChevronDown aria-hidden="true" />
                                </GlyphButton>
                                <GlyphButton
                                  label="移除目标"
                                  danger
                                  disabled={busy || (target.enabled && selected.enabled && enabledCount === 1)}
                                  onClick={() => void removeTarget(selected, target.upstreamModelId)}
                                >
                                  <Trash aria-hidden="true" />
                                </GlyphButton>
                              </div>
                            </li>
                          );
                        })}
                      </ol>
                    )}
                  </section>

                  {targets.length > 0 && !hasUsableBackup(targets) ? (
                    <InlineWarning message="没有备用目标，降级将不可用。" />
                  ) : null}
                  {actionError ? <InlineError message={actionError} /> : null}
                </>
              ) : (
                <EmptyNote>选择左侧的一条路由，或新建一个。</EmptyNote>
              )}
            </section>
          </div>
        )}
      </div>

      {basicsDraft ? (
        <RouteBasicsModal
          initial={basicsDraft}
          busy={busy}
          error={basicsError}
          onSubmit={(draft) => void submitBasics(draft)}
          onClose={() => {
            setBasicsDraft(null);
            setBasicsError(null);
          }}
        />
      ) : null}
    </main>
  );
}

