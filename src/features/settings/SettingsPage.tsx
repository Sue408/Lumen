import { useEffect, useRef, useState } from "react";
import { Database, Download, FlaskConical, Palette, RotateCcw, Server, Upload } from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import {
  InlineError,
  LoadingLines,
  SaveBar,
  SectionTitle,
  TogglePill,
} from "../../components/ConfigControls";
import { isTauriRuntime } from "../../components/tauriRuntime";
import { useGatewayStatus } from "../../app/useGatewayStatus";
import type { Theme } from "../../app/useTheme";
import {
  exportConfig,
  getAutostart,
  getSettings,
  importConfig,
  resetData,
  saveSettings,
  setAutostart as setAutostartEnabled,
  type ImportSummary,
} from "../../services/settings";
import { demoScenarios, injectDemo, type DemoScenario } from "../../services/demo";

type SettingsPageProps = {
  theme: Theme;
  onToggleTheme: () => void;
};

export function SettingsPage({ theme, onToggleTheme }: SettingsPageProps) {
  const gateway = useGatewayStatus();
  const running = gateway.status?.running ?? false;
  const [port, setPort] = useState("");
  const [savedPort, setSavedPort] = useState<number | null>(null);
  const [closeToTray, setCloseToTray] = useState(true);
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const closeToTraySaving = useRef(false);
  const autostartSaving = useRef(false);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [formError, setFormError] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [demoBusy, setDemoBusy] = useState(false);
  const [notice, setNotice] = useState<string | null>(null);
  const [confirmingReset, setConfirmingReset] = useState(false);
  const [pendingImport, setPendingImport] = useState<{
    path: string;
    summary: ImportSummary;
  } | null>(null);

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
        setCloseToTray(settings.closeToTray);
      } catch (err) {
        if (alive) setError(String(err));
      } finally {
        if (alive) setLoading(false);
      }
      try {
        const enabled = await getAutostart();
        if (alive) setAutostart(enabled);
      } catch (err) {
        if (alive) setError(String(err));
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
    try {
      const saved = await saveSettings({ port: value, closeToTray });
      setPort(String(saved.port));
      setSavedPort(saved.port);
      setCloseToTray(saved.closeToTray);
      setError(null);
      setNotice("端口已保存，下次启动生效");
    } catch (err) {
      setFormError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const toggleCloseToTray = async (next: boolean) => {
    if (savedPort === null || closeToTraySaving.current) return;
    closeToTraySaving.current = true;
    const previous = closeToTray;
    setCloseToTray(next);
    try {
      const saved = await saveSettings({ port: savedPort, closeToTray: next });
      setCloseToTray(saved.closeToTray);
      setError(null);
    } catch (err) {
      setCloseToTray(previous);
      setNotice(null);
      setError(String(err));
    } finally {
      closeToTraySaving.current = false;
    }
  };

  const toggleAutostart = async (next: boolean) => {
    if (autostartSaving.current) return;
    autostartSaving.current = true;
    const previous = autostart;
    setAutostart(next);
    try {
      const actual = await setAutostartEnabled(next);
      setAutostart(actual);
      setError(null);
    } catch (err) {
      setAutostart(previous);
      setNotice(null);
      setError(String(err));
    } finally {
      autostartSaving.current = false;
    }
  };

  const runExport = async () => {
    setError(null);
    setNotice(null);
    if (!isTauriRuntime(window)) {
      setNotice("浏览器环境不支持文件对话框");
      return;
    }
    try {
      const path = await save({
        title: "导出配置",
        defaultPath: "lumen.config.json",
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path) return;
      setBusy(true);
      await exportConfig(path);
      setNotice("已导出配置（含 API Key 与虚拟密钥明文，请妥善保管）");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const chooseImport = async () => {
    setError(null);
    setNotice(null);
    if (!isTauriRuntime(window)) {
      setNotice("浏览器环境不支持文件对话框");
      return;
    }
    try {
      const path = await open({
        title: "导入配置",
        multiple: false,
        filters: [{ name: "JSON", extensions: ["json"] }],
      });
      if (!path || Array.isArray(path)) return;
      setBusy(true);
      const summary = await importConfig(path, true);
      setPendingImport({ path, summary });
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
    }
  };

  const confirmImport = async () => {
    if (!pendingImport) return;
    setBusy(true);
    setError(null);
    try {
      await importConfig(pendingImport.path, false);
      setPendingImport(null);
      setNotice("配置已合并导入，切换页面后生效");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
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

  const runDemo = async (scenario: DemoScenario) => {
    setDemoBusy(true);
    setError(null);
    setNotice(null);
    try {
      const summary = await injectDemo(scenario);
      setNotice(
        summary
          ? `演示数据已替换：${summary.providers} 家上游 · ${summary.models} 个模型 · ${summary.routes} 条路由 · ${summary.virtualKeys} 个密钥 · ${summary.logs} 条流水（切换页面查看）`
          : "浏览器环境不支持注入",
      );
    } catch (err) {
      setError(String(err));
    } finally {
      setDemoBusy(false);
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

        <div className="settings-scroll">
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
                        onChange={(next) => void toggleAutostart(next)}
                      />
                    )}
                  </div>
                  <span className="settings-row-note">开机后自动运行，并以最小化方式静默进入托盘</span>
                </div>
                <div className="settings-row">
                  <span className="settings-row-label">关闭窗口时收进托盘</span>
                  <div className="settings-row-control">
                    <TogglePill
                      checked={closeToTray}
                      label="关闭窗口时收进托盘"
                      disabled={busy || savedPort === null}
                      onChange={(next) => void toggleCloseToTray(next)}
                    />
                  </div>
                  <span className="settings-row-note">
                    关闭按钮不退出，仅从任务栏隐藏；退出请用托盘菜单
                  </span>
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
                  <span className="settings-row-note">导出为 JSON，含 API Key 与虚拟密钥明文，请妥善保管</span>
                </div>
                <div className="settings-row">
                  <span className="settings-row-label">导入配置</span>
                  <div className="settings-row-control">
                    <button className="quiet-button" type="button" disabled={busy} onClick={() => void chooseImport()}>
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
                      onClick={() => void confirmImport()}
                    >
                      确认导入
                    </button>
                    <button
                      className="quiet-button"
                      type="button"
                      disabled={busy}
                      onClick={() => setPendingImport(null)}
                    >
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

            {import.meta.env.DEV ? (
              <section className="settings-section">
                <SectionTitle icon={<FlaskConical aria-hidden="true" />}>开发工具</SectionTitle>
                <div className="settings-list">
                  <div className="settings-row">
                    <span className="settings-row-label">演示数据</span>
                    <div className="settings-row-control demo-scenarios">
                      {demoScenarios.map((item) => (
                        <button
                          className="quiet-button"
                          key={item.key}
                          type="button"
                          title={item.hint}
                          disabled={demoBusy}
                          onClick={() => void runDemo(item.key)}
                        >
                          {item.label}
                        </button>
                      ))}
                    </div>
                    <span className="settings-row-note">
                      完全替换全部业务与用量数据，仅开发构建可见；注入后切换页面即可查看
                    </span>
                  </div>
                </div>
              </section>
            ) : null}
          </>
        )}
        </div>
      </div>
    </main>
  );
}
