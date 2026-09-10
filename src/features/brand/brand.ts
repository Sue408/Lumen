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

export const BRAND_LABELS: Record<BrandId, string> = {
  openai: "OpenAI",
  anthropic: "Anthropic",
  gemini: "Google Gemini",
  deepseek: "DeepSeek",
  mistral: "Mistral",
  meta: "Meta Llama",
  grok: "xAI Grok",
  qwen: "通义千问",
  moonshot: "月之暗面 Kimi",
  zhipu: "智谱 GLM",
  cohere: "Cohere",
  perplexity: "Perplexity",
  ollama: "Ollama",
  openrouter: "OpenRouter",
  groq: "Groq",
  together: "Together AI",
  fireworks: "Fireworks AI",
  huggingface: "Hugging Face",
  azure: "Azure OpenAI",
  bedrock: "AWS Bedrock",
  minimax: "MiniMax",
  baichuan: "百川智能",
  doubao: "字节豆包",
  zeroone: "零一万物",
  stepfun: "阶跃星辰",
  baidu: "百度文心",
  alibaba: "阿里云",
  internlm: "书生·浦语",
  nvidia: "NVIDIA",
  siliconcloud: "硅基流动",
  sensenova: "商汤日日新",
  hunyuan: "腾讯混元",
  spark: "讯飞星火",
};

const BRAND_ID_SET: ReadonlySet<string> = new Set(BRAND_IDS);

export function isBrandId(value: string | null | undefined): value is BrandId {
  return typeof value === "string" && BRAND_ID_SET.has(value);
}

/** 显式选择的图标优先，否则回落到关键词识别。 */
export function resolveBrand(
  icon: string | null | undefined,
  parts: Array<string | undefined | null>,
): BrandId | null {
  return isBrandId(icon) ? icon : detectBrand(parts);
}

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
