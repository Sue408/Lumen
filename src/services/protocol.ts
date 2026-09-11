export type Protocol = "openai" | "anthropic" | "responses" | "gemini";

export const protocolLabel: Record<Protocol, string> = {
  openai: "OpenAI Chat",
  anthropic: "Anthropic Messages",
  responses: "OpenAI Responses",
  gemini: "Gemini",
};
