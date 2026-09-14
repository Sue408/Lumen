export function moveTarget<T>(list: T[], index: number, delta: number): T[] {
  const next = index + delta;
  if (index < 0 || index >= list.length || next < 0 || next >= list.length) return [...list];
  const copy = [...list];
  [copy[index], copy[next]] = [copy[next], copy[index]];
  return copy;
}

/// 启用目标 ≥ 2 才具备降级备用。单目标可保存，但降级不可用（仅告警，不阻断）。
export function hasUsableBackup(targets: Array<{ enabled: boolean }>): boolean {
  return targets.filter((target) => target.enabled).length >= 2;
}

/// 隐式多协议后，路由对入站协议无约束，列表 / 详情统一展示此说明。
export const ROUTE_PROTOCOL_LABEL = "全协议";
