export const BRAND_IDS = [
  "openai",
  "anthropic",
  "gemini",
  "deepseek",
  "mistral",
  "meta",
  "grok",
  "qwen",
  "moonshot",
  "zhipu",
  "cohere",
  "perplexity",
  "ollama",
  "openrouter",
  "groq",
  "together",
  "fireworks",
  "huggingface",
  "azure",
  "bedrock",
  "minimax",
  "baichuan",
  "doubao",
  "zeroone",
  "stepfun",
  "baidu",
  "alibaba",
  "internlm",
  "nvidia",
  "siliconcloud",
  "sensenova",
  "hunyuan",
  "spark",
] as const;

export type BrandId = (typeof BRAND_IDS)[number];

const RULES: Array<[RegExp, BrandId]> = [
  [/deepseek/, "deepseek"],
  [/anthropic|claude/, "anthropic"],
  [/openai|gpt-|\bo1\b|\bo3\b|dall-e|text-embedding/, "openai"],
  [/gemini|google|palm|vertex|bard/, "gemini"],
  [/qwen|dashscope|tongyi|通义/, "qwen"],
  [/moonshot|kimi/, "moonshot"],
  [/zhipu|glm|智谱|bigmodel/, "zhipu"],
  [/mistral|mixtral|codestral|magistral/, "mistral"],
  [/ollama/, "ollama"],
  [/meta|llama/, "meta"],
  [/xai|grok/, "grok"],
  [/cohere|command-r/, "cohere"],
  [/perplexity|sonar/, "perplexity"],
  [/openrouter/, "openrouter"],
  [/\bgroq\b/, "groq"],
  [/together/, "together"],
  [/fireworks/, "fireworks"],
  [/hugging|hf\.co/, "huggingface"],
  [/azure/, "azure"],
  [/bedrock|\baws\b/, "bedrock"],
  [/minimax|abab/, "minimax"],
  [/baichuan|百川/, "baichuan"],
  [/doubao|volc|bytedance|豆包/, "doubao"],
  [/zeroone|01\.ai|lingyiwanwu|零一/, "zeroone"],
  [/stepfun|step-|阶跃/, "stepfun"],
  [/baidu|ernie|文心/, "baidu"],
  [/alibaba|aliyun|阿里/, "alibaba"],
  [/internlm|书生/, "internlm"],
  [/nvidia/, "nvidia"],
  [/siliconflow|silicon/, "siliconcloud"],
  [/sensenova|商汤/, "sensenova"],
  [/hunyuan|tencent|混元|腾讯/, "hunyuan"],
  [/spark|iflytek|讯飞/, "spark"],
];

export function detectBrand(parts: Array<string | undefined | null>): BrandId | null {
  const haystack = parts.filter(Boolean).join(" ").toLowerCase();
  if (haystack.length === 0) return null;
  for (const [pattern, brand] of RULES) {
    if (pattern.test(haystack)) return brand;
  }
  return null;
}
