import { useEffect, useState } from "react";
import { ChartColumn, Coins, KeyRound, Plus, Trash } from "lucide-react";
import {
  EmptyNote,
  GlyphButton,
  InlineError,
  LoadingLines,
  SaveBar,
  SectionTitle,
  StatusDot,
  TogglePill,
} from "../../components/ConfigControls";
import { RegisterList } from "../../components/RegisterList";
import {
  deleteVirtualKey,
  listVirtualKeys,
  queryVirtualKeyUsage,
  saveVirtualKey,
  type KeyUsage,
  type QuotaPeriod,
  type VirtualKey,
} from "../../services/config";
import {
  emptyKeyDraft,
  isKeyDraftDirty,
  keyToDraft,
  maskKey,
  quotaConfigSummary,
  quotaLimitToNumber,
  quotaPeriodLabel,
  quotaPeriodOrder,
  quotaRatio,
  quotaSummary,
  quotaTone,
  validateKeyDraft,
  type KeyDraft,
} from "./keyModel";

type Pending = { kind: "existing"; id: string } | { kind: "new" };

function KeyMark({ enabled }: { enabled: boolean }) {
  return (
    <span className={enabled ? "brand-mark" : "brand-mark is-off"} aria-hidden="true">
      <KeyRound />
    </span>
  );
}

function KeyForm({
  draft,
  savedDraft,
  busy,
  error,
  usage,
  onChange,
  onDiscard,
  onSubmit,
}: {
  draft: KeyDraft;
  savedDraft: KeyDraft;
  busy: boolean;
  error: string | null;
  usage: KeyUsage | null;
  onChange: (draft: KeyDraft) => void;
  onDiscard: () => void;
  onSubmit: () => void;
}) {
  const [reveal, setReveal] = useState(false);
  const [copyState, setCopyState] = useState<"idle" | "ok" | "fail">("idle");
  const set = (patch: Partial<KeyDraft>) => onChange({ ...draft, ...patch });
  const dirty = isKeyDraftDirty(draft, savedDraft);
  const tone = usage ? quotaTone(usage.spent, usage.limit) : "normal";

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(draft.key);
      setCopyState("ok");
    } catch {
      setCopyState("fail");
    }
    window.setTimeout(() => setCopyState("idle"), 1600);
  };

  const rotate = () => {
    set({ key: `sk-lumen-${crypto.randomUUID().replace(/-/g, "")}` });
  };

  const copyLabel = copyState === "ok" ? "已复制" : copyState === "fail" ? "复制失败" : "复制";

  return (
    <form
      className="entry-form"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <section className="sheet-section">
        <SectionTitle icon={<KeyRound aria-hidden="true" />}>基础信息</SectionTitle>
        <div className="field-grid">
          <label className="field">
            <span>名称</span>
            <input
              value={draft.name}
              onChange={(event) => set({ name: event.target.value })}
              placeholder="例如 Claude 桌面端"
            />
          </label>
          <div className="field field-wide">
            <span>密钥</span>
            <div className="key-value-row">
              <code className="key-value" title={reveal ? draft.key : undefined}>
                {reveal ? draft.key : maskKey(draft.key)}
              </code>
              <button className="text-action" type="button" onClick={() => setReveal((value) => !value)}>
                {reveal ? "隐藏" : "显示"}
              </button>
              <button className="text-action" type="button" onClick={() => void copy()} disabled={busy}>
                {copyLabel}
              </button>
              <button className="text-action" type="button" onClick={rotate} disabled={busy}>
                轮换
              </button>
            </div>
          </div>
        </div>
      </section>

      <section className="sheet-section">
        <SectionTitle icon={<Coins aria-hidden="true" />}>额度</SectionTitle>
        <div className="field-grid">
          <div className="field">
            <span>计量</span>
            <span className="field-static">按花费 ¥</span>
          </div>
          <label className="field">
            <span>上限 · 元 / 留空不限</span>
            <input
              inputMode="decimal"
              value={draft.quotaLimit}
              onChange={(event) => set({ quotaLimit: event.target.value })}
              placeholder="不限"
            />
          </label>
          <label className="field">
            <span>周期</span>
            <select
              value={draft.quotaPeriod}
              onChange={(event) => set({ quotaPeriod: event.target.value as QuotaPeriod })}
            >
              {quotaPeriodOrder.map((period) => (
                <option key={period} value={period}>
                  {quotaPeriodLabel[period]}
                </option>
              ))}
            </select>
          </label>
        </div>
      </section>

      {draft.id ? (
        <section className="sheet-section">
          <SectionTitle icon={<ChartColumn aria-hidden="true" />}>本期分户</SectionTitle>
          {usage ? (
            <>
              <p className="sheet-summary">
                <span>{quotaSummary(usage.spent, usage.limit, usage.period)}</span>
                <span className="sheet-summary-sep" aria-hidden="true">·</span>
                <span>{usage.calls} 次调用</span>
                {usage.limit !== null ? (
                  <>
                    <span className="sheet-summary-sep" aria-hidden="true">·</span>
                    <span className={`quota-state is-${tone}`}>
                      {tone === "over" ? "已超限" : tone === "near" ? "临近上限" : "额度充足"}
                    </span>
                  </>
                ) : null}
              </p>
              {usage.limit !== null ? (
                <div className={`quota-meter is-${tone}`} aria-hidden="true">
                  <span
                    className="quota-meter-fill"
                    style={{ width: `${quotaRatio(usage.spent, usage.limit) * 100}%` }}
                  />
                </div>
              ) : null}
            </>
          ) : (
            <p className="sheet-summary">正在读取分户数据…</p>
          )}
        </section>
      ) : null}

      {error ? <InlineError message={error} /> : null}
      <SaveBar dirty={dirty} busy={busy} label={draft.id ? "保存密钥" : "创建密钥"} onDiscard={onDiscard} />
    </form>
  );
}

export function KeysPage() {
  const [keys, setKeys] = useState<VirtualKey[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [formError, setFormError] = useState<string | null>(null);
  const [selectedId, setSelectedId] = useState<string | "new" | null>(null);
  const [draft, setDraft] = useState<KeyDraft | null>(null);
  const [savedDraft, setSavedDraft] = useState<KeyDraft | null>(null);
  const [usage, setUsage] = useState<KeyUsage | null>(null);
  const [pending, setPending] = useState<Pending | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const dirty = draft !== null && savedDraft !== null && isKeyDraftDirty(draft, savedDraft);

  const loadUsage = async (id: string) => {
    try {
      const next = await queryVirtualKeyUsage(id);
      setUsage(next);
    } catch {
      setUsage(null);
    }
  };

  const select = (key: VirtualKey) => {
    const next = keyToDraft(key);
    setSelectedId(key.id);
    setDraft(next);
    setSavedDraft({ ...next });
    setUsage(null);
    setFormError(null);
    setConfirmingDelete(false);
    setPending(null);
    void loadUsage(key.id);
  };

  const selectNew = () => {
    setSelectedId("new");
    setDraft(emptyKeyDraft());
    setSavedDraft(emptyKeyDraft());
    setUsage(null);
    setFormError(null);
    setConfirmingDelete(false);
    setPending(null);
  };

  const requestSelect = (key: VirtualKey) => {
    if (dirty) setPending({ kind: "existing", id: key.id });
    else select(key);
  };

  const requestNew = () => {
    if (dirty) setPending({ kind: "new" });
    else selectNew();
  };

  const refreshLists = async () => {
    const nextKeys = await listVirtualKeys();
    setKeys(nextKeys);
    return nextKeys;
  };

  useEffect(() => {
    let alive = true;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const nextKeys = await listVirtualKeys();
        if (!alive) return;
        setKeys(nextKeys);
        if (nextKeys.length > 0) select(nextKeys[0]);
        else selectNew();
      } catch (err) {
        if (alive) setError(String(err));
      } finally {
        if (alive) setLoading(false);
      }
    })();
    return () => {
      alive = false;
    };
  }, []);

  const submit = async (): Promise<boolean> => {
    if (!draft) return false;
    const message = validateKeyDraft(draft);
    if (message) {
      setFormError(message);
      return false;
    }
    setBusy(true);
    setFormError(null);
    try {
      const saved = await saveVirtualKey({
        id: draft.id,
        key: draft.key || null,
        name: draft.name.trim(),
        enabled: draft.enabled,
        quotaLimit: quotaLimitToNumber(draft.quotaLimit),
        quotaPeriod: draft.quotaPeriod,
      });
      await refreshLists();
      const next = keyToDraft(saved);
      setSelectedId(saved.id);
      setDraft(next);
      setSavedDraft({ ...next });
      void loadUsage(saved.id);
      return true;
    } catch (err) {
      setFormError(String(err));
      return false;
    } finally {
      setBusy(false);
    }
  };

  const discardDraft = () => {
    if (!savedDraft) return;
    setDraft({ ...savedDraft });
    setFormError(null);
  };

  const applyPending = () => {
    if (!pending) return;
    if (pending.kind === "new") selectNew();
    else {
      const key = keys.find((item) => item.id === pending.id);
      if (key) select(key);
      else setPending(null);
    }
  };

  const saveAndSwitch = async () => {
    const ok = await submit();
    if (ok) applyPending();
  };

  const deleteSelected = async () => {
    if (!selectedId || selectedId === "new") return;
    setBusy(true);
    setError(null);
    try {
      await deleteVirtualKey(selectedId);
      const nextKeys = await refreshLists();
      setConfirmingDelete(false);
      if (nextKeys.length > 0) select(nextKeys[0]);
      else selectNew();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="keys-page">
      <div className="config-column">
        <header className="page-header">
          <div>
            <div className="eyebrow">VIRTUAL KEYS</div>
            <h1>虚拟密钥</h1>
          </div>
        </header>

        {error ? <InlineError message={error} /> : null}

        {loading ? (
          <LoadingLines rows={5} />
        ) : (
          <div className="config-workbench">
            <section className="register-pane">
              <div className="register-head">
                <span className="register-title">虚拟密钥 · {keys.length} 个</span>
                <GlyphButton label="新建密钥" onClick={requestNew}>
                  <Plus aria-hidden="true" />
                </GlyphButton>
              </div>

              {keys.length === 0 ? (
                <div className="register-empty">
                  <EmptyNote>还没有虚拟密钥，网关将拒绝所有调用。</EmptyNote>
                  <button className="text-action" type="button" onClick={requestNew}>
                    创建第一个密钥
                  </button>
                </div>
              ) : (
                <RegisterList
                  ariaLabel="虚拟密钥列表"
                  selectedKey={selectedId === "new" ? null : selectedId}
                >
                  {keys.map((key) => (
                    <button
                      className={`register-select${selectedId === key.id ? " is-selected" : ""}${key.enabled ? "" : " is-off"}`}
                      type="button"
                      key={key.id}
                      data-register-key={key.id}
                      aria-current={selectedId === key.id ? "true" : undefined}
                      onClick={() => requestSelect(key)}
                    >
                      <KeyMark enabled={key.enabled} />
                      <span className="register-body">
                        <span className="register-name" title={key.name}>
                          {key.name}
                        </span>
                        <span className="register-meta">
                          {quotaConfigSummary(key.quotaLimit, key.quotaPeriod)}
                        </span>
                      </span>
                      <StatusDot alive={key.enabled} />
                    </button>
                  ))}
                </RegisterList>
              )}
            </section>

            <section className="workbench-sheet" aria-label="虚拟密钥编辑">
              {draft ? (
                <>
                  <div className="sheet-head">
                    <KeyMark enabled={draft.enabled} />
                    <h2 className="sheet-title">
                      {selectedId === "new" ? "新建密钥" : draft.name || "未命名密钥"}
                    </h2>
                    <TogglePill
                      checked={draft.enabled}
                      label="启用该密钥"
                      disabled={busy}
                      onChange={(next) => setDraft({ ...draft, enabled: next })}
                    />
                    <div className="sheet-actions">
                      {selectedId !== "new" ? (
                        <GlyphButton
                          label="删除该密钥"
                          danger
                          disabled={busy}
                          onClick={() => setConfirmingDelete(true)}
                        >
                          <Trash aria-hidden="true" />
                        </GlyphButton>
                      ) : null}
                    </div>
                  </div>

                  {pending ? (
                    <div className="pending-bar">
                      <span>有未保存的修改，切换前要保存吗？</span>
                      <button
                        className="quiet-button is-primary"
                        type="button"
                        onClick={() => void saveAndSwitch()}
                        disabled={busy}
                      >
                        保存并切换
                      </button>
                      <button className="quiet-button" type="button" onClick={applyPending} disabled={busy}>
                        放弃并切换
                      </button>
                      <button className="quiet-button" type="button" onClick={() => setPending(null)} disabled={busy}>
                        取消
                      </button>
                    </div>
                  ) : null}

                  {confirmingDelete ? (
                    <div className="confirm-bar">
                      <span>确认移除该虚拟密钥？使用它的客户端将立即失去访问。</span>
                      <button
                        className="quiet-button is-danger"
                        type="button"
                        onClick={() => void deleteSelected()}
                        disabled={busy}
                      >
                        移除
                      </button>
                      <button
                        className="quiet-button"
                        type="button"
                        onClick={() => setConfirmingDelete(false)}
                        disabled={busy}
                      >
                        取消
                      </button>
                    </div>
                  ) : null}

                  <KeyForm
                    key={selectedId ?? "new"}
                    draft={draft}
                    savedDraft={savedDraft ?? draft}
                    busy={busy}
                    error={formError}
                    usage={usage}
                    onChange={setDraft}
                    onDiscard={discardDraft}
                    onSubmit={() => void submit()}
                  />
                </>
              ) : null}
            </section>
          </div>
        )}
      </div>
    </main>
  );
}
