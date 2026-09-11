import { protocolLabel, type Protocol } from "../../services/protocol.ts";

export type ProtocolEndpoint = {
  protocol: Protocol;
  label: string;
  method: "POST";
  path: string;
  note?: string;
};

/// 各协议入站端点的对外说明。路径与 `gateway/handlers.rs`、`docs/后端接口文档.md` 对应；
/// 新增协议时必须同步本表（测试会守住覆盖率）。
export const protocolEndpoints: ProtocolEndpoint[] = [
  {
    protocol: "openai",
    label: protocolLabel.openai,
    method: "POST",
    path: "/v1/chat/completions",
    note: "别名填在 model",
  },
  {
    protocol: "anthropic",
    label: protocolLabel.anthropic,
    method: "POST",
    path: "/v1/messages",
    note: "别名填在 model",
  },
  {
    protocol: "responses",
    label: protocolLabel.responses,
    method: "POST",
    path: "/v1/responses",
    note: "别名填在 model",
  },
  {
    protocol: "gemini",
    label: protocolLabel.gemini,
    method: "POST",
    path: "/v1beta/models/{别名}:generateContent",
    note: "别名在路径；流式用 streamGenerateContent",
  },
];
