import { Database, Download, RotateCcw, Upload } from "lucide-react";
import { SectionTitle } from "../../components/ConfigControls";
import type { ImportSummary } from "../../services/settings";

type DataSectionProps = {
  busy: boolean;
  running: boolean;
  pendingImport: { path: string; summary: ImportSummary } | null;
  confirmingReset: boolean;
  onExport: () => void;
  onChooseImport: () => void;
  onConfirmImport: () => void;
  onCancelImport: () => void;
  onRequestReset: () => void;
  onConfirmReset: () => void;
  onCancelReset: () => void;
};

export function DataSection({
  busy,
  running,
  pendingImport,
  confirmingReset,
  onExport,
  onChooseImport,
  onConfirmImport,
  onCancelImport,
  onRequestReset,
  onConfirmReset,
  onCancelReset,
}: DataSectionProps) {
  return (
    <section className="settings-section">
      <SectionTitle icon={<Database aria-hidden="true" />}>数据</SectionTitle>
      <div className="settings-list">
        <div className="settings-row">
          <span className="settings-row-label">导出配置</span>
          <div className="settings-row-control">
            <button className="quiet-button" type="button" disabled={busy} onClick={onExport}>
              <Download aria-hidden="true" />
              导出
            </button>
          </div>
          <span className="settings-row-note">导出为 JSON，含 API Key 与虚拟密钥明文，请妥善保管</span>
        </div>
        <div className="settings-row">
          <span className="settings-row-label">导入配置</span>
          <div className="settings-row-control">
            <button className="quiet-button" type="button" disabled={busy} onClick={onChooseImport}>
              <Upload aria-hidden="true" />
              导入
            </button>
          </div>
          <span className="settings-row-note">合并覆盖：同名提供商 / 模型 / 路由 / 密钥按文件更新，未提及的保留</span>
        </div>
        {pendingImport ? (
          <div className="confirm-bar">
            <span>
              将新增：提供商 {pendingImport.summary.providers.created}、模型{" "}
              {pendingImport.summary.models.created}、路由 {pendingImport.summary.routes.created}、
              密钥 {pendingImport.summary.virtualKeys.created}；覆盖：提供商{" "}
              {pendingImport.summary.providers.updated}、模型 {pendingImport.summary.models.updated}、
              路由 {pendingImport.summary.routes.updated}、密钥{" "}
              {pendingImport.summary.virtualKeys.updated}。确认导入？
            </span>
            <button
              className="quiet-button is-primary"
              type="button"
              disabled={busy}
              onClick={onConfirmImport}
            >
              确认导入
            </button>
            <button className="quiet-button" type="button" disabled={busy} onClick={onCancelImport}>
              取消
            </button>
          </div>
        ) : null}
        <div className="settings-row">
          <span className="settings-row-label">重置数据</span>
          <div className="settings-row-control">
            <button
              className="quiet-button is-danger"
              type="button"
              disabled={busy || running}
              onClick={onRequestReset}
            >
              <RotateCcw aria-hidden="true" />
              重置
            </button>
          </div>
          <span className="settings-row-note">
            {running ? "需先停止网关" : "清空上游、模型、路由、密钥、日志，保留设置"}
          </span>
        </div>
        {confirmingReset ? (
          <div className="confirm-bar">
            <span>将清空全部业务数据，此操作不可撤销。确认继续？</span>
            <button className="quiet-button is-danger" type="button" disabled={busy} onClick={onConfirmReset}>
              确认重置
            </button>
            <button className="quiet-button" type="button" disabled={busy} onClick={onCancelReset}>
              取消
            </button>
          </div>
        ) : null}
      </div>
    </section>
  );
}
