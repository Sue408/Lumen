import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../components/tauriRuntime";

export type DemoScenario = "rich" | "empty" | "errors" | "unassigned" | "spike" | "solo";

export type DemoSummary = {
  providers: number;
  models: number;
  routes: number;
  virtualKeys: number;
  logs: number;
};

export const demoScenarios: { key: DemoScenario; label: string; hint: string }[] = [
  { key: "rich", label: "丰富", hint: "宣传主场景：数据健康、有起伏" },
  { key: "errors", label: "高失败", hint: "失败与用量存疑偏多" },
  { key: "unassigned", label: "未归属", hint: "大量未归属调用" },
  { key: "spike", label: "尖峰", hint: "近三天用量暴涨" },
  { key: "solo", label: "单模型", hint: "仅一个上游与密钥" },
  { key: "empty", label: "空数据", hint: "清空全部业务数据" },
];

/** 开发专用：完全替换业务数据。生产构建下后端不注册该命令。 */
export async function injectDemo(scenario: DemoScenario): Promise<DemoSummary | null> {
  if (!isTauriRuntime(window)) return null;
  return invoke<DemoSummary>("inject_demo_cmd", { scenario });
}
