import { type ReactNode } from "react";
import azure from "@lobehub/icons-static-svg/icons/azure.svg?raw";
import baichuan from "@lobehub/icons-static-svg/icons/baichuan.svg?raw";
import baidu from "@lobehub/icons-static-svg/icons/baidu.svg?raw";
import bedrock from "@lobehub/icons-static-svg/icons/bedrock.svg?raw";
import alibaba from "@lobehub/icons-static-svg/icons/alibaba.svg?raw";
import anthropic from "@lobehub/icons-static-svg/icons/anthropic.svg?raw";
import cohere from "@lobehub/icons-static-svg/icons/cohere.svg?raw";
import deepseek from "@lobehub/icons-static-svg/icons/deepseek.svg?raw";
import doubao from "@lobehub/icons-static-svg/icons/doubao.svg?raw";
import fireworks from "@lobehub/icons-static-svg/icons/fireworks.svg?raw";
import gemini from "@lobehub/icons-static-svg/icons/gemini.svg?raw";
import grok from "@lobehub/icons-static-svg/icons/grok.svg?raw";
import groq from "@lobehub/icons-static-svg/icons/groq.svg?raw";
import huggingface from "@lobehub/icons-static-svg/icons/huggingface.svg?raw";
import hunyuan from "@lobehub/icons-static-svg/icons/hunyuan.svg?raw";
import internlm from "@lobehub/icons-static-svg/icons/internlm.svg?raw";
import meta from "@lobehub/icons-static-svg/icons/meta.svg?raw";
import minimax from "@lobehub/icons-static-svg/icons/minimax.svg?raw";
import mistral from "@lobehub/icons-static-svg/icons/mistral.svg?raw";
import moonshot from "@lobehub/icons-static-svg/icons/moonshot.svg?raw";
import nvidia from "@lobehub/icons-static-svg/icons/nvidia.svg?raw";
import ollama from "@lobehub/icons-static-svg/icons/ollama.svg?raw";
import openai from "@lobehub/icons-static-svg/icons/openai.svg?raw";
import openrouter from "@lobehub/icons-static-svg/icons/openrouter.svg?raw";
import perplexity from "@lobehub/icons-static-svg/icons/perplexity.svg?raw";
import qwen from "@lobehub/icons-static-svg/icons/qwen.svg?raw";
import sensenova from "@lobehub/icons-static-svg/icons/sensenova.svg?raw";
import siliconcloud from "@lobehub/icons-static-svg/icons/siliconcloud.svg?raw";
import spark from "@lobehub/icons-static-svg/icons/spark.svg?raw";
import stepfun from "@lobehub/icons-static-svg/icons/stepfun.svg?raw";
import together from "@lobehub/icons-static-svg/icons/together.svg?raw";
import zeroone from "@lobehub/icons-static-svg/icons/zeroone.svg?raw";
import zhipu from "@lobehub/icons-static-svg/icons/zhipu.svg?raw";
import type { BrandId } from "./brand";

const BRAND_SVG: Record<BrandId, string> = {
  openai,
  anthropic,
  gemini,
  deepseek,
  mistral,
  meta,
  grok,
  qwen,
  moonshot,
  zhipu,
  cohere,
  perplexity,
  ollama,
  openrouter,
  groq,
  together,
  fireworks,
  huggingface,
  azure,
  bedrock,
  minimax,
  baichuan,
  doubao,
  zeroone,
  stepfun,
  baidu,
  alibaba,
  internlm,
  nvidia,
  siliconcloud,
  sensenova,
  hunyuan,
  spark,
};

export function BrandGlyph({
  brand,
  size = 20,
  fallback,
  className,
}: {
  brand: BrandId | null;
  size?: number;
  fallback: ReactNode;
  className?: string;
}) {
  const classes = className ? `brand-mark ${className}` : "brand-mark";
  if (!brand) {
    return (
      <span className={`${classes} brand-mark-fallback`} style={{ fontSize: `${size}px` }} aria-hidden="true">
        {fallback}
      </span>
    );
  }
  return (
    <span
      className={classes}
      style={{ fontSize: `${size}px` }}
      aria-hidden="true"
      dangerouslySetInnerHTML={{ __html: BRAND_SVG[brand] }}
    />
  );
}
