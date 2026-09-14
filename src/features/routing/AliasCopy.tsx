import { useState } from "react";
import { Check, Copy } from "lucide-react";

/** 把路由别名复制到剪贴板，短暂反馈「已复制」；写入失败则静默复位。 */
export function AliasCopy({ alias, disabled }: { alias: string; disabled?: boolean }) {
  const [copied, setCopied] = useState(false);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(alias);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  return (
    <button
      className="alias-copy"
      type="button"
      title={copied ? "已复制" : "复制别名"}
      aria-label="复制别名"
      disabled={disabled}
      onClick={() => void copy()}
    >
      {copied ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
    </button>
  );
}
