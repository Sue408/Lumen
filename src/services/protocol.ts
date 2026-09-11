export type Protocol = "openai" | "anthropic";

export const protocolLabel: Record<Protocol, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
};
