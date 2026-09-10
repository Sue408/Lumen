import type { RouteWithTargets } from "../../services/config";

export function moveTarget<T>(list: T[], index: number, delta: number): T[] {
  const next = index + delta;
  if (index < 0 || index >= list.length || next < 0 || next >= list.length) return [...list];
  const copy = [...list];
  [copy[index], copy[next]] = [copy[next], copy[index]];
  return copy;
}

export type RouteTargetDraft = {
  uid: string;
  upstreamModelId: string;
  enabled: boolean;
};

export type RouteDraft = {
  id: string | null;
  alias: string;
  displayName: string;
  enabled: boolean;
  targets: RouteTargetDraft[];
};

export function makeTarget(upstreamModelId: string): RouteTargetDraft {
  return { uid: crypto.randomUUID(), upstreamModelId, enabled: true };
}

export function emptyRouteDraft(): RouteDraft {
  return { id: null, alias: "", displayName: "", enabled: true, targets: [] };
}

export function routeToDraft(route: RouteWithTargets): RouteDraft {
  return {
    id: route.id,
    alias: route.alias,
    displayName: route.displayName,
    enabled: route.enabled,
    targets: [...route.targets]
      .sort((a, b) => a.priority - b.priority)
      .map((target) => ({
        uid: crypto.randomUUID(),
        upstreamModelId: target.upstreamModelId,
        enabled: target.enabled,
      })),
  };
}

export function isRouteDraftDirty(draft: RouteDraft, original: RouteDraft): boolean {
  if (draft.alias !== original.alias) return true;
  if (draft.displayName !== original.displayName) return true;
  if (draft.enabled !== original.enabled) return true;
  if (draft.targets.length !== original.targets.length) return true;
  return draft.targets.some((target, index) => {
    const reference = original.targets[index];
    return (
      target.upstreamModelId !== reference.upstreamModelId ||
      target.enabled !== reference.enabled
    );
  });
}

export function validateRouteDraft(draft: RouteDraft): string | null {
  const alias = draft.alias.trim();
  if (alias.length === 0) return "请填写路由别名。";
  if (/\s/.test(alias)) return "别名不能包含空格。";
  if (draft.targets.length === 0) return "请至少指定一个上游目标。";
  if (draft.targets.some((target) => target.upstreamModelId.length === 0)) {
    return "每个目标都需要选择上游模型。";
  }
  const seen = new Set<string>();
  for (const target of draft.targets) {
    if (seen.has(target.upstreamModelId)) return "同一个上游模型不能重复添加。";
    seen.add(target.upstreamModelId);
  }
  return null;
}
