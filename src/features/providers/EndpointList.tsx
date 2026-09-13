import { useRef, useState } from "react";
import { Network, Pencil, Plus, Trash2 } from "lucide-react";
import { GlyphButton, SectionTitle } from "../../components/ConfigControls";
import { Select } from "../../components/Select";
import {
  authSchemeLabel,
  authSchemeOrder,
  endpointHost,
  nextEndpointDraft,
  protocolLabel,
  protocolOrder,
  validateEndpointDraft,
  type EndpointDraft,
} from "./providerModel";
import { EndpointHelp } from "./EndpointHelp";
import type { ProviderEndpoint, ProviderEndpointInput } from "../../services/config";

function toDraft(endpoints: ProviderEndpoint[]): EndpointDraft[] {
  return endpoints.map((endpoint) => ({
    id: endpoint.id,
    protocol: endpoint.protocol,
    baseUrl: endpoint.baseUrl,
    authScheme: endpoint.authScheme,
  }));
}

function toInput(rows: EndpointDraft[]): ProviderEndpointInput[] {
  return rows.map((row) => ({
    id: row.id,
    protocol: row.protocol,
    baseUrl: row.baseUrl,
    authScheme: row.authScheme,
  }));
}

/**
 * 协议端点编辑器。端点数量少、字段少，故**静默保存**：改动即回写，文本在失焦时回写，
 * 不占用整页的保存栏。行内编辑用本地草稿，非法地址不落库、也不允许收起。
 */
export function EndpointList({
  endpoints,
  busy,
  onSave,
}: {
  endpoints: ProviderEndpoint[];
  busy: boolean;
  onSave: (endpoints: ProviderEndpointInput[]) => void;
}) {
  const [rows, setRows] = useState<EndpointDraft[]>(() => toDraft(endpoints));
  const [editingIndex, setEditingIndex] = useState<number | null>(null);
  const rowsRef = useRef(rows);
  rowsRef.current = rows;
  const committedRef = useRef(JSON.stringify(toInput(rows)));
  const allProtocolsUsed = rows.length >= protocolOrder.length;

  const commit = (next: EndpointDraft[]) => {
    const input = toInput(next);
    committedRef.current = JSON.stringify(input);
    setRows(next);
    onSave(input);
  };

  const patchLocal = (index: number, change: Partial<EndpointDraft>) =>
    setRows((previous) =>
      previous.map((row, current) => (current === index ? { ...row, ...change } : row)),
    );

  const patchCommit = (index: number, change: Partial<EndpointDraft>) =>
    commit(rowsRef.current.map((row, current) => (current === index ? { ...row, ...change } : row)));

  const commitBaseUrl = (index: number) => {
    if (validateEndpointDraft(rowsRef.current[index])) return;
    const input = toInput(rowsRef.current);
    const snapshot = JSON.stringify(input);
    if (snapshot === committedRef.current) return;
    committedRef.current = snapshot;
    onSave(input);
  };

  const add = () => {
    const next = [...rowsRef.current, nextEndpointDraft(rowsRef.current)];
    commit(next);
    setEditingIndex(next.length - 1);
  };

  const remove = (index: number) => {
    commit(rowsRef.current.filter((_, current) => current !== index));
    setEditingIndex((previous) => {
      if (previous === null || previous === index) return null;
      return previous > index ? previous - 1 : previous;
    });
  };

  return (
    <section className="sheet-section">
      <SectionTitle
        icon={<Network aria-hidden="true" />}
        action={
          <button
            className="text-action"
            type="button"
            onClick={add}
            disabled={busy || allProtocolsUsed}
            title={allProtocolsUsed ? "四种协议端点均已配置" : undefined}
          >
            <Plus aria-hidden="true" />
            添加协议端点
          </button>
        }
      >
        协议端点
        <EndpointHelp />
      </SectionTitle>
      <div className="endpoint-list">
        {rows.map((row, index) => {
          const editing = editingIndex === index;
          const endpointError = editing ? validateEndpointDraft(row) : null;
          return (
            <div
              className={editing ? "endpoint-item is-editing" : "endpoint-item"}
              key={row.id ?? `new-${index}`}
            >
              <div className="endpoint-item-head">
                <span className="endpoint-item-protocol">{protocolLabel[row.protocol]}</span>
                <code className="endpoint-item-host" title={row.baseUrl}>
                  {row.baseUrl.trim() ? endpointHost(row.baseUrl) : "未填写地址"}
                </code>
                <div className="endpoint-item-actions">
                  <GlyphButton
                    label={editing ? "收起该端点" : "编辑该端点"}
                    disabled={busy || (editing && endpointError !== null)}
                    onClick={() => setEditingIndex(editing ? null : index)}
                  >
                    <Pencil aria-hidden="true" />
                  </GlyphButton>
                  <GlyphButton
                    label="删除该端点"
                    danger
                    disabled={busy || rows.length <= 1}
                    onClick={() => remove(index)}
                  >
                    <Trash2 aria-hidden="true" />
                  </GlyphButton>
                </div>
              </div>
              {editing ? (
                <div className="endpoint-item-body">
                  <label className="field">
                    <span>协议</span>
                    <Select
                      label="协议"
                      value={row.protocol}
                      options={protocolOrder.map((protocol) => ({
                        value: protocol,
                        label: protocolLabel[protocol],
                        disabled: rows.some(
                          (other, otherIndex) =>
                            otherIndex !== index && other.protocol === protocol,
                        ),
                      }))}
                      onChange={(protocol) => patchCommit(index, { protocol })}
                    />
                  </label>
                  <label className="field">
                    <span>上游地址</span>
                    <input
                      value={row.baseUrl}
                      disabled={busy}
                      onChange={(event) => patchLocal(index, { baseUrl: event.target.value })}
                      onBlur={() => commitBaseUrl(index)}
                      placeholder="https://api.example.com/v1"
                      spellCheck={false}
                    />
                  </label>
                  <label className="field endpoint-auth">
                    <span>鉴权方式</span>
                    <Select
                      label="鉴权方式"
                      value={row.authScheme}
                      options={authSchemeOrder.map((scheme) => ({
                        value: scheme,
                        label: authSchemeLabel[scheme],
                      }))}
                      onChange={(authScheme) => patchCommit(index, { authScheme })}
                    />
                  </label>
                  {endpointError ? <p className="field-error">{endpointError}</p> : null}
                  <div className="endpoint-item-foot">
                    <button
                      className="text-action"
                      type="button"
                      disabled={busy || endpointError !== null}
                      title={endpointError ?? undefined}
                      onClick={() => setEditingIndex(null)}
                    >
                      收起
                    </button>
                  </div>
                </div>
              ) : null}
            </div>
          );
        })}
      </div>
    </section>
  );
}
