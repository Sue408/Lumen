import {
  ChartColumn,
  KeyRound,
  Route,
  ScrollText,
  Server,
  Settings,
  type LucideIcon,
} from "lucide-react";

export type NavigationItem = {
  id: string;
  label: string;
  icon: LucideIcon;
};

export const navigationItems = [
  { id: "usage", label: "用量统计", icon: ChartColumn },
  { id: "routing", label: "模型路由", icon: Route },
  { id: "keys", label: "虚拟密钥", icon: KeyRound },
  { id: "providers", label: "上游提供商", icon: Server },
  { id: "logs", label: "使用日志", icon: ScrollText },
  { id: "settings", label: "设置", icon: Settings },
] as const satisfies readonly NavigationItem[];

export type ViewId = (typeof navigationItems)[number]["id"];
