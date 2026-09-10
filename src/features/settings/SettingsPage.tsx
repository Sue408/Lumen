import { useEffect, useState } from "react";
import { Database, Download, FolderOpen, Palette, RotateCcw, Server } from "lucide-react";
import { revealItemInDir } from "@tauri-apps/plugin-opener";
import { InlineError, LoadingLines, SaveBar, SectionTitle } from "../../components/ConfigControls";
import { isTauriRuntime } from "../../components/tauriRuntime";
import { useGatewayStatus } from "../../app/useGatewayStatus";
import type { Theme } from "../../app/useTheme";
import { exportSeed, getSettings, resetData, saveSettings } from "../../services/settings";

type SettingsPageProps = {
  theme: Theme;
  onToggleTheme: () => void;
};

export function SettingsPage({ theme, onToggleTheme }: SettingsPageProps) {
  const gateway = useGatewayStatus();
  const running = gateway.status?.running ?? false;
  const [port, setPort] = useState("");
  const [savedPort, setSavedPort] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [exportPath, setExportPath] = useState<string | null>(null);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmingReset, setConfirmingReset] = useState(false);

  const dirty = savedPort !== null && port.trim() !== String(savedPort);

  useEffect(() => {
    let alive = true;
    (async () => {
      setLoading(true);
      setError(null);
      try {
        const settings = await getSettings();
        if (!alive) return;
        setPort(String(settings.port));
        setSavedPort(settings.port);
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

  const submitPort = async () => {
    const value = Number(port.trim());
    if (!Number.isInteger(value) || value < 1024 || value > 65535) {
      setFormError("端口需在 1024–65535 之间");
      return;
    }
    setBusy(true);
    setFormError(null);
    setNotice(null);
    try {
      const saved = await saveSettings({ port: value });
      setPort(String(saved.port));
      setSavedPort(saved.port);
      setNotice("端口已保存，下次启动生效");
    } catch (err) {
      setFormError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const runExport = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      const path = await exportSeed();
      setExportPath(path);
      setNotice("已导出当前配置");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const revealExport = async () => {
    if (!exportPath || !isTauriRuntime(window)) return;
    try {
      await revealItemInDir(exportPath);
    } catch (err) {
      setError(String(err));
    }
  };

  const runReset = async () => {
    setBusy(true);
    setError(null);
    setNotice(null);
    try {
      await resetData();
      setConfirmingReset(false);
      setNotice("已清空业务数据");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  return (
    <main className="settings-page">
      <div className="settings-column">
        <header className="page-header">
          <div>
            <div className="eyebrow">SETTINGS</div>
            <h1>设置</h1>
          </div>
        </header>

        {error ? <InlineError message={error} /> : null}
        {notice ? (
          <p className="settings-notice" role="status">
            {notice}
          </p>
        ) : null}

        {loading ? (
          <LoadingLines rows={4} />
        ) : (
          <>
            <form
              className="settings-section"
              onSubmit={(event) => {
                event.preventDefault();
                void submitPort();
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
                    onChange={(event) => {
                      setPort(event.target.value);
                      setFormError(null);
                      setNotice(null);
                    }}
                  />
                  <span className="settings-row-note">
                    {running ? "网关运行中，需先停止后才能修改" : "回环地址 127.0.0.1，范围 1024–65535"}
                  </span>
                </div>
                <div className="settings-row">
                  <span className="settings-row-label">随系统启动</span>
                  <div className="settings-row-control">
                    <button
                      className="toggle-pill is-pending"
                      type="button"
                      role="switch"
                      aria-checked={false}
                      aria-label="随系统启动（待接入）"
                      title="待接入"
                      disabled
                    >
                      <span className="status-dot" aria-hidden="true" />
                      待接入
                    </button>
                  </div>
                  <span className="settings-row-note">功能完善后接入</span>
                </div>
              </div>
              {formError ? <InlineError message={formError} /> : null}
              <SaveBar
                dirty={dirty}
                busy={busy}
                label="保存端口"
                onDiscard={() => {
                  setPort(String(savedPort ?? ""));
                  setFormError(null);
                }}
              />
            </form>

            <section className="settings-section">
              <SectionTitle icon={<Palette aria-hidden="true" />}>外观</SectionTitle>
              <div className="settings-list">
                <div className="settings-row">
                  <span className="settings-row-label">主题</span>
                  <div className="settings-row-control">
                    <div className="settings-segment" role="group" aria-label="主题">
                      <button
                        className={theme === "light" ? "is-selected" : ""}
                        type="button"
                        aria-pressed={theme === "light"}
                        onClick={() => theme !== "light" && onToggleTheme()}
                      >
                        浅色
                      </button>
                      <button
                        className={theme === "dark" ? "is-selected" : ""}
                        type="button"
                        aria-pressed={theme === "dark"}
                        onClick={() => theme !== "dark" && onToggleTheme()}
                      >
                        深色
                      </button>
                    </div>
                  </div>
                  <span className="settings-row-note">随本机保存，重启后沿用</span>
                </div>
              </div>
            </section>

            <section className="settings-section">
              <SectionTitle icon={<Database aria-hidden="true" />}>数据</SectionTitle>
              <div className="settings-list">
                <div className="settings-row">
                  <span className="settings-row-label">导出配置</span>
                  <div className="settings-row-control">
                    <button className="quiet-button" type="button" disabled={busy} onClick={() => void runExport()}>
                      <Download aria-hidden="true" />
                      导出
                    </button>
                  </div>
                  <span className="settings-row-note">写入应用数据目录的 lumen.seed.json</span>
                </div>
                {exportPath ? (
                  <div className="settings-row">
                    <span className="settings-row-label settings-path" title={exportPath}>
                      {exportPath}
                    </span>
                    <div className="settings-row-control">
                      <button className="quiet-button" type="button" onClick={() => void revealExport()}>
                        <FolderOpen aria-hidden="true" />
                        打开所在文件夹
                      </button>
                    </div>
                  </div>
                ) : null}
                <div className="settings-row">
                  <span className="settings-row-label">重置数据</span>
                  <div className="settings-row-control">
                    <button
                      className="quiet-button is-danger"
                      type="button"
                      disabled={busy || running}
                      onClick={() => setConfirmingReset(true)}
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
                    <button className="quiet-button is-danger" type="button" disabled={busy} onClick={() => void runReset()}>
                      确认重置
                    </button>
                    <button className="quiet-button" type="button" disabled={busy} onClick={() => setConfirmingReset(false)}>
                      取消
                    </button>
                  </div>
                ) : null}
              </div>
            </section>
          </>
        )}
      </div>
    </main>
  );
}
