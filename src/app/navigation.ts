export type ViewId =
  | "usage"
  | "routing"
  | "keys"
  | "providers"
  | "logs"
  | "settings";

export type NavigationItem = {
  id: ViewId;
  label: string;
};

export const navigationItems: NavigationItem[] = [
  { id: "usage", label: "用量统计" },
  { id: "routing", label: "模型路由" },
  { id: "keys", label: "虚拟密钥" },
  { id: "providers", label: "上游提供商" },
  { id: "logs", label: "使用日志" },
  { id: "settings", label: "设置" },
];
