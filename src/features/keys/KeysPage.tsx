import { useEffect, useState } from "react";
import { useLiveRevision } from "../../app/useLiveRevision";
import { Check, Copy, KeyRound, Pencil, Plus, Trash } from "lucide-react";
import {
  EmptyNote,
  GlyphButton,
  InlineError,
  LoadingLines,
  TogglePill,
} from "../../components/ConfigControls";
import { Modal } from "../../components/Modal";
import {
  deleteVirtualKey,
  listVirtualKeys,
  queryVirtualKeysUsage,
  saveVirtualKey,
  type KeyUsage,
  type QuotaPeriod,
  type VirtualKey,
} from "../../services/config";
import {
  emptyKeyDraft,
  keyToDraft,
  maskKey,
  quotaAmount,
  quotaLimitToNumber,
  quotaPeriodLabel,
  quotaPeriodOrder,
  quotaRatio,
  quotaResetSummary,
  quotaTone,
  validateKeyDraft,
  type KeyDraft,
} from "./keyModel";

type Editor = { mode: "create" } | { mode: "edit"; id: string };

function KeyMark({ enabled }: { enabled: boolean }) {
  return (
    <span className={enabled ? "brand-mark" : "brand-mark is-off"} aria-hidden="true">
      <KeyRound />
    </span>
  );
}

function KeyEditor({
  title,
  draft,
  busy,
  error,
  onChange,
  onClose,
  onSubmit,
}: {
  title: string;
  draft: KeyDraft;
  busy: boolean;
  error: string | null;
  onChange: (draft: KeyDraft) => void;
  onClose: () => void;
  onSubmit: () => void;
}) {
  const set = (patch: Partial<KeyDraft>) => onChange({ ...draft, ...patch });
  const isCreate = draft.id === null;

  return (
    <Modal title={title} onClose={onClose}>
      <form
        className="entry-form"
        onSubmit={(event) => {
          event.preventDefault();
          onSubmit();
        }}
      >
        <div className="field-grid">
          <label className="field field-wide">
            <span>名称</span>
            <input
              value={draft.name}
              onChange={(event) => set({ name: event.target.value })}
              placeholder="例如 Claude 桌面端"
            />
          </label>
          <label className="field">
            <span>上限 · $ / 留空不限</span>
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

        {error ? <InlineError message={error} /> : null}

        <div className="form-actions">
          <button className="quiet-button is-primary" type="submit" disabled={busy}>
            {busy ? "保存中…" : isCreate ? "创建密钥" : "保存"}
          </button>
          <button className="quiet-button" type="button" onClick={onClose} disabled={busy}>
            取消
          </button>
        </div>
      </form>
    </Modal>
  );
}

export function KeysPage() {
  const [keys, setKeys] = useState<VirtualKey[]>([]);
  const [usages, setUsages] = useState<Record<string, KeyUsage>>({});
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);

  const [editor, setEditor] = useState<Editor | null>(null);
  const [draft, setDraft] = useState<KeyDraft | null>(null);
  const [formError, setFormError] = useState<string | null>(null);

  const [copyState, setCopyState] = useState<Record<string, "ok" | "fail" | undefined>>({});
  const [confirmDelete, setConfirmDelete] = useState<string | null>(null);

  const { revision } = useLiveRevision();

  const fetchUsages = async (): Promise<Record<string, KeyUsage>> => {
    const list = await queryVirtualKeysUsage();
    const map: Record<string, KeyUsage> = {};
    for (const usage of list) map[usage.keyId] = usage;
    return map;
  };

  // 密钥与用量一起取回、一起落状态，登记簿首帧就是完整的——否则先画空壳、
  // 用量随后补进来，行会在进入页面的一瞬间跳一下。
  const reload = async (): Promise<VirtualKey[]> => {
    const [next, usageMap] = await Promise.all([listVirtualKeys(), fetchUsages()]);
    setKeys(next);
    setUsages(usageMap);
    return next;
  };

  useEffect(() => {
    let alive = true;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const [next, usageMap] = await Promise.all([listVirtualKeys(), fetchUsages()]);
        if (!alive) return;
        setKeys(next);
        setUsages(usageMap);
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

  useEffect(() => {
    if (revision === 0) return;
    void fetchUsages().then(setUsages);
    // live usage should refresh with each new gateway request
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [revision]);

  const openCreate = () => {
    setDraft(emptyKeyDraft());
    setEditor({ mode: "create" });
    setFormError(null);
  };

  const openEdit = (key: VirtualKey) => {
    setDraft(keyToDraft(key));
    setEditor({ mode: "edit", id: key.id });
    setFormError(null);
  };

  const closeEditor = () => {
    setEditor(null);
    setDraft(null);
    setFormError(null);
  };

  const submit = async () => {
    if (!draft) return;
    const message = validateKeyDraft(draft);
    if (message) {
      setFormError(message);
      return;
    }
    const isCreate = draft.id === null;
    setBusy(true);
    setFormError(null);
    try {
      await saveVirtualKey({
        id: draft.id,
        key: isCreate ? null : draft.key || null,
        name: draft.name.trim(),
        enabled: draft.enabled,
        quotaLimit: quotaLimitToNumber(draft.quotaLimit),
        quotaPeriod: draft.quotaPeriod,
      });
      await reload();
      closeEditor();
    } catch (err) {
      setFormError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const toggleEnabled = async (key: VirtualKey) => {
    setBusy(true);
    setError(null);
    try {
      await saveVirtualKey({
        id: key.id,
        name: key.name,
        enabled: !key.enabled,
        quotaLimit: key.quotaLimit,
        quotaPeriod: key.quotaPeriod,
      });
      await reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const remove = async (id: string) => {
    setBusy(true);
    setError(null);
    try {
      await deleteVirtualKey(id);
      setConfirmDelete(null);
      await reload();
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const copy = async (key: VirtualKey) => {
    try {
      await navigator.clipboard.writeText(key.key);
      setCopyState((prev) => ({ ...prev, [key.id]: "ok" }));
    } catch {
      setCopyState((prev) => ({ ...prev, [key.id]: "fail" }));
    }
    window.setTimeout(() => setCopyState((prev) => ({ ...prev, [key.id]: undefined })), 1600);
  };

  const enabledCount = keys.filter((key) => key.enabled).length;
  const now = new Date();

  return (
    <main className="keys-page">
      <div className="keys-column">
        <header className="page-header">
          <div>
            <div className="eyebrow">VIRTUAL KEYS</div>
            <h1>虚拟密钥</h1>
          </div>
        </header>

        {error ? <InlineError message={error} /> : null}

        {loading ? (
          <LoadingLines rows={4} />
        ) : keys.length === 0 ? (
          <div className="keys-empty">
            <EmptyNote>还没有虚拟密钥，网关将拒绝所有调用。</EmptyNote>
            <button className="quiet-button is-primary keys-new" type="button" onClick={openCreate}>
              <Plus aria-hidden="true" />
              创建第一个密钥
            </button>
          </div>
        ) : (
          <section className="keys-register">
            <div className="keys-controls">
              <p className="keys-summary">
                {keys.length} 个密钥 · <strong>{enabledCount}</strong> 个启用
              </p>
              <button className="quiet-button is-primary keys-new" type="button" onClick={openCreate}>
                <Plus aria-hidden="true" />
                新建密钥
              </button>
            </div>
            <ul className="keys-list">
              {keys.map((key) => {
                const usage = usages[key.id] ?? null;
                const tone = usage ? quotaTone(usage.spent, usage.limit) : "normal";
                const copied = copyState[key.id];
                return (
                  <li className={`key-card${key.enabled ? "" : " is-off"}`} key={key.id}>
                    <span className="key-part key-part-id">
                      <KeyMark enabled={key.enabled} />
                      <span className="key-card-name" title={key.name}>
                        {key.name}
                      </span>
                    </span>

                    <span className="key-part key-part-key">
                      <span className="key-mask" aria-hidden="true">
                        {maskKey(key.key)}
                      </span>
                      <button
                        className="key-copy"
                        type="button"
                        title={copied === "ok" ? "已复制" : "复制密钥"}
                        aria-label="复制密钥"
                        disabled={busy}
                        onClick={() => void copy(key)}
                      >
                        {copied === "ok" ? <Check aria-hidden="true" /> : <Copy aria-hidden="true" />}
                      </button>
                    </span>

                    <span className="key-part key-part-quota">
                      {usage ? (
                        <>
                          <span className="quota-line">
                            {usage.limit !== null ? (
                              <>
                                <span className={`quota-meter is-${tone}`} aria-hidden="true">
                                  <span
                                    className="quota-meter-fill"
                                    style={{
                                      width: `${quotaRatio(usage.spent, usage.limit) * 100}%`,
                                    }}
                                  />
                                </span>
                                <span className={`key-quota-text is-${tone}`}>
                                  {quotaAmount(usage.spent, usage.limit)}
                                </span>
                              </>
                            ) : (
                              <>
                                <span className="quota-meter is-unlimited" aria-hidden="true" />
                                <span className="key-quota-text">
                                  {quotaAmount(usage.spent, null)}
                                </span>
                              </>
                            )}
                          </span>
                          <span className="key-quota-reset">
                            {quotaResetSummary(usage.period, usage.periodStart, now)}
                          </span>
                        </>
                      ) : (
                        <>
                          <span className="quota-line">
                            <span className="quota-meter is-pending" aria-hidden="true" />
                            <span className="key-quota-text is-pending">--</span>
                          </span>
                          <span className="key-quota-reset">读取中…</span>
                        </>
                      )}
                    </span>

                    <span className="key-part key-part-actions">
                      <TogglePill
                        checked={key.enabled}
                        label={key.enabled ? "停用该密钥" : "启用该密钥"}
                        small
                        disabled={busy}
                        onChange={() => void toggleEnabled(key)}
                      />
                      <GlyphButton label="编辑密钥" disabled={busy} onClick={() => openEdit(key)}>
                        <Pencil aria-hidden="true" />
                      </GlyphButton>
                      <GlyphButton
                        label="删除密钥"
                        danger
                        disabled={busy}
                        onClick={() => setConfirmDelete(key.id)}
                      >
                        <Trash aria-hidden="true" />
                      </GlyphButton>
                    </span>

                    {confirmDelete === key.id ? (
                      <div className="confirm-bar">
                        <span>确认移除该虚拟密钥？使用它的客户端将立即失去访问。</span>
                        <button
                          className="quiet-button is-danger"
                          type="button"
                          onClick={() => void remove(key.id)}
                          disabled={busy}
                        >
                          移除
                        </button>
                        <button
                          className="quiet-button"
                          type="button"
                          onClick={() => setConfirmDelete(null)}
                          disabled={busy}
                        >
                          取消
                        </button>
                      </div>
                    ) : null}
                  </li>
                );
              })}
            </ul>
          </section>
        )}
      </div>

      {editor && draft ? (
        <KeyEditor
          title={editor.mode === "create" ? "新建虚拟密钥" : "编辑虚拟密钥"}
          draft={draft}
          busy={busy}
          error={formError}
          onChange={setDraft}
          onClose={closeEditor}
          onSubmit={() => void submit()}
        />
      ) : null}
    </main>
  );
}
