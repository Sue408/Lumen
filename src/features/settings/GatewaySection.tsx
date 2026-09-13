import { Server } from "lucide-react";
import { InlineError, SaveBar, SectionTitle, TogglePill } from "../../components/ConfigControls";

type GatewaySectionProps = {
  port: string;
  savedPort: number | null;
  proxyUrl: string;
  savedProxyUrl: string;
  running: boolean;
  autostart: boolean | null;
  autostartGateway: boolean;
  closeToTray: boolean;
  busy: boolean;
  formError: string | null;
  onPortChange: (value: string) => void;
  onProxyChange: (value: string) => void;
  onSubmitPort: () => void;
  onDiscardPort: () => void;
  onToggleAutostart: (next: boolean) => void;
  onToggleAutostartGateway: (next: boolean) => void;
  onToggleCloseToTray: (next: boolean) => void;
};

export function GatewaySection({
  port,
  savedPort,
  proxyUrl,
  savedProxyUrl,
  running,
  autostart,
  autostartGateway,
  closeToTray,
  busy,
  formError,
  onPortChange,
  onProxyChange,
  onSubmitPort,
  onDiscardPort,
  onToggleAutostart,
  onToggleAutostartGateway,
  onToggleCloseToTray,
}: GatewaySectionProps) {
  const dirty =
    savedPort !== null &&
    (port.trim() !== String(savedPort) || proxyUrl.trim() !== savedProxyUrl.trim());

  return (
    <form
      className="settings-section"
      onSubmit={(event) => {
        event.preventDefault();
        onSubmitPort();
      }}
    >
      <SectionTitle icon={<Server aria-hidden="true" />}>网关</SectionTitle>
      <div className="settings-list">
        <div className="settings-row">
          <label className="settings-row-label" htmlFor="settings-port">
            监听端口
          </label>
          <input
            id="settings-port"
            className="settings-input"
            inputMode="numeric"
            value={port}
            disabled={running || busy}
            onChange={(event) => onPortChange(event.target.value)}
          />
          <span className="settings-row-note">
            {running ? "网关运行中，需先停止后才能修改" : "回环地址 127.0.0.1，范围 1024–65535"}
          </span>
        </div>
        <div className="settings-row">
          <label className="settings-row-label" htmlFor="settings-proxy">
            出站代理
          </label>
          <input
            id="settings-proxy"
            className="settings-input settings-input-url"
            type="text"
            placeholder="http://127.0.0.1:7890"
            value={proxyUrl}
            disabled={busy}
            spellCheck={false}
            onChange={(event) => onProxyChange(event.target.value)}
          />
          <span className="settings-row-note">
            留空则直连；支持 http/https 代理，保存后即时生效
          </span>
        </div>
        <div className="settings-row">
          <span className="settings-row-label">随系统启动</span>
          <div className="settings-row-control">
            {autostart === null ? (
              <button
                className="toggle-pill is-pending"
                type="button"
                role="switch"
                aria-checked={false}
                aria-label="随系统启动（读取中）"
                title="读取中"
                disabled
              >
                <span className="status-dot" aria-hidden="true" />
                读取中
              </button>
            ) : (
              <TogglePill
                checked={autostart}
                label="随系统启动"
                disabled={busy}
                onChange={onToggleAutostart}
              />
            )}
          </div>
          <span className="settings-row-note">开机后自动运行，并以最小化方式静默进入托盘</span>
        </div>
        <div className="settings-row">
          <span className="settings-row-label">自启时自动开启网关</span>
          <div className="settings-row-control">
            <TogglePill
              checked={autostartGateway}
              label="自启时自动开启网关"
              disabled={busy || autostart !== true}
              onChange={onToggleAutostartGateway}
            />
          </div>
          <span className="settings-row-note">
            {autostart === true
              ? "随系统启动时无需手动开启，网关自动就绪"
              : "需先开启「随系统启动」"}
          </span>
        </div>
        <div className="settings-row">
          <span className="settings-row-label">关闭窗口时收进托盘</span>
          <div className="settings-row-control">
            <TogglePill
              checked={closeToTray}
              label="关闭窗口时收进托盘"
              disabled={busy || savedPort === null}
              onChange={onToggleCloseToTray}
            />
          </div>
          <span className="settings-row-note">关闭按钮不退出，仅从任务栏隐藏；退出请用托盘菜单</span>
        </div>
      </div>
      {formError ? <InlineError message={formError} /> : null}
      <SaveBar dirty={dirty} busy={busy} label="保存网关设置" onDiscard={onDiscardPort} />
    </form>
  );
}
