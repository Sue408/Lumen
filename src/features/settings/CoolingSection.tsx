import { useCallback, useEffect, useState } from "react";
import { Snowflake } from "lucide-react";
import { InlineError, SectionTitle } from "../../components/ConfigControls";
import { listUpstreamModels } from "../../services/config";
import { clearCooling, queryTelemetry, type CoolingDetail } from "../../services/telemetry";

/// 冷却类目 → 人话。键是后端 `ErrorKind` 的 snake_case。
const REASON_LABELS: Record<string, string> = {
  link_connect: "无法连接上游",
  link_timeout: "连接上游超时",
  link_stream_reset: "上游响应中断",
  link_stream_stalled: "上游长时间无数据",
  upstream_unavailable: "上游不可用",
  upstream_rate_limited: "上游限流",
  upstream_bad_response: "上游返回异常",
  upstream_stream_mismatch: "上游返回了非流式响应",
  upstream_truncated: "上游流未正常收尾",
};

const reasonLabel = (kind: string) => REASON_LABELS[kind] ?? kind;

/**
 * 上游冷却：当前哪些上游被降级链临时跳过、因何、还剩多久，并提供手动解除。
 *
 * 「解除」是逃生阀而非修复——上游仍不可用时下一次请求会立刻重新冷却，所以文案要说清。
 */
export function CoolingSection() {
  const [details, setDetails] = useState<CoolingDetail[]>([]);
  const [names, setNames] = useState<Map<string, string>>(new Map());
  const [elapsed, setElapsed] = useState(0);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    try {
      const snapshot = await queryTelemetry("day");
      setDetails(snapshot.coolingDetail);
      setElapsed(0);
      setError(null);
    } catch (err) {
      setError(String(err));
    }
  }, []);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  // 模型名只为可读性服务，拿不到就退回 id，不阻塞主流程。
  useEffect(() => {
    let alive = true;
    listUpstreamModels()
      .then((models) => {
        if (alive) setNames(new Map(models.map((model) => [model.id, model.displayName])));
      })
      .catch(() => undefined);
    return () => {
      alive = false;
    };
  }, []);

  // 本地倒计时：后端只给快照不推送，不递减的话「剩余 37s」会一直停在那儿骗人。
  useEffect(() => {
    if (details.length === 0) return;
    const timer = window.setInterval(() => setElapsed((value) => value + 1), 1000);
    return () => window.clearInterval(timer);
  }, [details.length]);

  const live = details.filter((detail) => detail.remainingSecs > elapsed);

  const clear = async (model?: string) => {
    setBusy(true);
    try {
      setDetails(await clearCooling(model));
      setElapsed(0);
      setError(null);
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="settings-section">
      <SectionTitle icon={<Snowflake aria-hidden="true" />}>上游冷却</SectionTitle>
      <div className="settings-list">
        {live.length === 0 ? (
          <div className="settings-row">
            <span className="settings-row-label">冷却中</span>
            <div className="settings-row-control">
              <span className="cooling-remaining">无</span>
            </div>
            <span className="settings-row-note">
              上游连续失败后会被短暂冷却，成功一次即刻解除
            </span>
          </div>
        ) : (
          live.map((detail) => (
            <div className="settings-row" key={detail.upstreamModelId}>
              <span className="settings-row-label">
                {names.get(detail.upstreamModelId) ?? detail.upstreamModelId}
              </span>
              <div className="settings-row-control">
                <span className="cooling-remaining">
                  {detail.remainingSecs - elapsed}s
                </span>
                <button
                  className="quiet-button"
                  type="button"
                  disabled={busy}
                  onClick={() => void clear(detail.upstreamModelId)}
                >
                  解除
                </button>
              </div>
              <span className="settings-row-note">
                {reasonLabel(detail.errorKind)} · 解除不等于修复，上游仍不可用时会立刻重新冷却
              </span>
            </div>
          ))
        )}
        {live.length > 1 ? (
          <div className="settings-row">
            <span className="settings-row-label">全部解除</span>
            <div className="settings-row-control">
              <button
                className="quiet-button"
                type="button"
                disabled={busy}
                onClick={() => void clear()}
              >
                全部解除
              </button>
            </div>
            <span className="settings-row-note">清空全部冷却，下一次请求会直接重试所有上游</span>
          </div>
        ) : null}
        {error ? <InlineError message={error} /> : null}
      </div>
    </section>
  );
}
