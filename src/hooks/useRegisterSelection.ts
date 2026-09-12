import { useState } from "react";

export type PendingSwitch = { kind: "existing"; id: string } | { kind: "new" };

type Entity = { id: string };

/**
 * 配置登记簿的选择/切换编排：持有当前草稿、已保存草稿、脏标记，
 * 以及「有未保存修改时切换需确认」的 pending 流。实体与草稿的转换由调用方提供。
 */
export function useRegisterSelection<T extends Entity, D>(options: {
  entities: T[];
  toDraft: (entity: T) => D;
  emptyDraft: () => D;
  isDirty: (draft: D, savedDraft: D) => boolean;
  onSelect?: () => void;
}) {
  const { entities, toDraft, emptyDraft, isDirty, onSelect } = options;
  const [selectedId, setSelectedId] = useState<string | "new" | null>(null);
  const [draft, setDraft] = useState<D | null>(null);
  const [savedDraft, setSavedDraft] = useState<D | null>(null);
  const [pending, setPending] = useState<PendingSwitch | null>(null);
  const [confirmingDelete, setConfirmingDelete] = useState(false);

  const dirty = draft !== null && savedDraft !== null && isDirty(draft, savedDraft);

  const select = (entity: T) => {
    setSelectedId(entity.id);
    setDraft(toDraft(entity));
    setSavedDraft(toDraft(entity));
    setPending(null);
    setConfirmingDelete(false);
    onSelect?.();
  };

  const selectNew = () => {
    setSelectedId("new");
    setDraft(emptyDraft());
    setSavedDraft(emptyDraft());
    setPending(null);
    setConfirmingDelete(false);
    onSelect?.();
  };

  const requestSelect = (entity: T) => {
    if (dirty) setPending({ kind: "existing", id: entity.id });
    else select(entity);
  };

  const requestNew = () => {
    if (dirty) setPending({ kind: "new" });
    else selectNew();
  };

  const applyPending = () => {
    if (!pending) return;
    if (pending.kind === "new") {
      selectNew();
      return;
    }
    const entity = entities.find((item) => item.id === pending.id);
    if (entity) select(entity);
    else setPending(null);
  };

  const saveAndSwitch = async (save: () => Promise<boolean>) => {
    if (await save()) applyPending();
  };

  return {
    selectedId,
    draft,
    savedDraft,
    dirty,
    pending,
    confirmingDelete,
    setDraft,
    setSavedDraft,
    setSelectedId,
    setPending,
    setConfirmingDelete,
    select,
    selectNew,
    requestSelect,
    requestNew,
    applyPending,
    saveAndSwitch,
  };
}
