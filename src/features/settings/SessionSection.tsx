import { useEffect, useState } from "react";
import { Inbox } from "lucide-react";
import { InlineError, SaveBar, SectionTitle } from "../../components/ConfigControls";
import { DEFAULT_SESSION_HEADERS } from "../../services/settings";

type SessionSectionProps = {
  headers: string[];
  busy: boolean;
  onSave: (headers: string[]) => void;
};

function parseHeaders(text: string): string[] {
  return text
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter((line) => line.length > 0);
}

function headersToText(headers: string[]): string {
  return headers.join("\n");
}

/**
 * 会话候选头名表：按优先级从上到下取值，客户端没带任何候选头时该次请求会话为空。
 * 留空即关闭会话捕获。
 */
export function SessionSection({ headers, busy, onSave }: SessionSectionProps) {
  const [text, setText] = useState(headersToText(headers));
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setText(headersToText(headers));
    setError(null);
  }, [headers]);

  const parsed = parseHeaders(text);
  const dirty = headersToText(parsed) !== headersToText(headers);

  const save = () => {
    const invalid = parsed.find((name) => /\s/.test(name));
    if (invalid) {
      setError(`头名不能包含空白：${invalid}`);
      return;
    }
    setError(null);
    onSave(parsed);
  };

  return (
    <form
      className="settings-section"
      onSubmit={(event) => {
        event.preventDefault();
        save();
      }}
    >
      <SectionTitle icon={<Inbox aria-hidden="true" />}>会话识别头</SectionTitle>
      <div className="settings-list">
        <div className="settings-row">
          <label className="settings-row-label" htmlFor="settings-session-headers">
            候选头名
          </label>
          <textarea
            id="settings-session-headers"
            className="settings-textarea"
            rows={5}
            value={text}
            disabled={busy}
            spellCheck={false}
            onChange={(event) => setText(event.target.value)}
          />
          <span className="settings-row-note">
            每行一个，按优先级自上而下取第一个非空值；留空则不记录会话。默认覆盖 Claude
            Code / Codex / OpenCode。
          </span>
          <button
            className="text-action"
            type="button"
            disabled={busy}
            onClick={() => setText(headersToText(DEFAULT_SESSION_HEADERS))}
          >
            恢复默认
          </button>
        </div>
      </div>
      {error ? <InlineError message={error} /> : null}
      <SaveBar dirty={dirty} busy={busy} label="保存会话头" onDiscard={() => setText(headersToText(headers))} />
    </form>
  );
}
