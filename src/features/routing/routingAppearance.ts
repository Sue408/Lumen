import { isBrandId, resolveBrand, type BrandId } from "../brand/brand";
import type { IconTint, Provider, UpstreamModel } from "../../services/config";

export type ModelMark = { brand: BrandId | null; tint: IconTint; enabled: boolean };

export function buildBrandLookup(
  providers: Provider[],
  models: UpstreamModel[],
): Map<string, ModelMark> {
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

export function markTint(mark: ModelMark | null | undefined): IconTint {
  return mark && mark.enabled ? mark.tint : "ink";
}

/** 按 priority 取首选目标的模型标记。 */
export function primaryModelMark(
  targets: Array<{ upstreamModelId: string; priority: number }>,
  brands: Map<string, ModelMark>,
): ModelMark | null {
  if (targets.length === 0) return null;
  const primary = [...targets].sort((a, b) => a.priority - b.priority)[0];
  return brands.get(primary.upstreamModelId) ?? null;
}

/** 图标优先自动取「首选目标上游模型」的品牌；着色未显式指定时继承该模型。 */
export function routeAppearance(
  fields: { icon: string | null; iconTint: IconTint | null; enabled: boolean },
  mark: ModelMark | null,
): { brand: BrandId | null; auto: BrandId | null; tint: IconTint } {
  const auto = mark?.brand ?? null;
  const brand = isBrandId(fields.icon) ? fields.icon : auto;
  const tint: IconTint = fields.enabled ? fields.iconTint ?? mark?.tint ?? "ink" : "ink";
  return { brand, auto, tint };
}
