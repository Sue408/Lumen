import { useEffect, useRef, useState } from "react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { InlineError, LoadingLines } from "../../components/ConfigControls";
import { isTauriRuntime } from "../../components/tauriRuntime";
import { useGatewayStatus } from "../../app/useGatewayStatus";
import type { Theme } from "../../app/useTheme";
import {
  DEFAULT_SESSION_HEADERS,
  exportConfig,
  getAppVersion,
  getAutostart,
  getAutostartGateway,
  getSettings,
  importConfig,
  resetData,
  saveSettings,
  setAutostart as setAutostartEnabled,
  setAutostartGateway,
  type ImportSummary,
} from "../../services/settings";
import { injectDemo, type DemoScenario } from "../../services/demo";
import { AppearanceSection } from "./AppearanceSection";
import { DataSection } from "./DataSection";
import { DevToolsSection } from "./DevToolsSection";
import { GatewaySection } from "./GatewaySection";
import { SessionSection } from "./SessionSection";

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
  const [sessionHeaders, setSessionHeaders] = useState<string[]>([...DEFAULT_SESSION_HEADERS]);
  const [proxyUrl, setProxyUrl] = useState("");
  const [savedProxyUrl, setSavedProxyUrl] = useState("");
  const [autostart, setAutostart] = useState<boolean | null>(null);
  const [autostartGateway, setAutostartGatewayState] = useState(false);
  const [appVersion, setAppVersion] = useState("");
  const closeToTraySaving = useRef(false);
  const autostartSaving = useRef(false);
  const autostartGatewaySaving = useRef(false);
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
        setSessionHeaders(settings.sessionHeaders ?? [...DEFAULT_SESSION_HEADERS]);
        setProxyUrl(settings.proxyUrl ?? "");
        setSavedProxyUrl(settings.proxyUrl ?? "");
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
      try {
        const enabled = await getAutostartGateway();
        if (alive) setAutostartGatewayState(enabled);
      } catch (err) {
        if (alive) setError(String(err));
      }
      try {
        const version = await getAppVersion();
        if (alive) setAppVersion(version);
      } catch {
        // 版本展示失败不影响设置页其余功能。
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
      const saved = await saveSettings({
        port: value,
        closeToTray,
        sessionHeaders,
        proxyUrl: proxyUrl.trim() || null,
      });
      setPort(String(saved.port));
      setSavedPort(saved.port);
      setCloseToTray(saved.closeToTray);
      setSessionHeaders(saved.sessionHeaders);
      setProxyUrl(saved.proxyUrl ?? "");
      setSavedProxyUrl(saved.proxyUrl ?? "");
      setError(null);
      setNotice("网关设置已保存：端口下次启动生效，代理即时生效");
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
      const saved = await saveSettings({
        port: savedPort,
        closeToTray: next,
        sessionHeaders,
        proxyUrl: savedProxyUrl.trim() || null,
      });
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

  const saveSessionHeaders = async (headers: string[]) => {
    if (savedPort === null) return;
    setBusy(true);
    try {
      const saved = await saveSettings({
        port: savedPort,
        closeToTray,
        sessionHeaders: headers,
        proxyUrl: savedProxyUrl.trim() || null,
      });
      setSessionHeaders(saved.sessionHeaders);
      setError(null);
      setNotice("会话识别头已保存");
    } catch (err) {
      setError(String(err));
    } finally {
      setBusy(false);
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

  const toggleAutostartGateway = async (next: boolean) => {
    if (autostartGatewaySaving.current) return;
    autostartGatewaySaving.current = true;
    const previous = autostartGateway;
    setAutostartGatewayState(next);
    try {
      const actual = await setAutostartGateway(next);
      setAutostartGatewayState(actual);
      setError(null);
    } catch (err) {
      setAutostartGatewayState(previous);
      setNotice(null);
      setError(String(err));
    } finally {
      autostartGatewaySaving.current = false;
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

  const handlePortChange = (value: string) => {
    setPort(value);
    setFormError(null);
    setNotice(null);
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
              <GatewaySection
                port={port}
                savedPort={savedPort}
                proxyUrl={proxyUrl}
                savedProxyUrl={savedProxyUrl}
                running={running}
                autostart={autostart}
                autostartGateway={autostartGateway}
                closeToTray={closeToTray}
                busy={busy}
                formError={formError}
                onPortChange={handlePortChange}
                onProxyChange={(value) => {
                  setProxyUrl(value);
                  setFormError(null);
                  setNotice(null);
                }}
                onSubmitPort={() => void submitPort()}
                onDiscardPort={() => {
                  setPort(String(savedPort ?? ""));
                  setProxyUrl(savedProxyUrl);
                  setFormError(null);
                }}
                onToggleAutostart={(next) => void toggleAutostart(next)}
                onToggleAutostartGateway={(next) => void toggleAutostartGateway(next)}
                onToggleCloseToTray={(next) => void toggleCloseToTray(next)}
              />

              <AppearanceSection theme={theme} onToggleTheme={onToggleTheme} />

              <SessionSection
                headers={sessionHeaders}
                busy={busy}
                onSave={(headers) => void saveSessionHeaders(headers)}
              />

              <DataSection
                busy={busy}
                running={running}
                pendingImport={pendingImport}
                confirmingReset={confirmingReset}
                onExport={() => void runExport()}
                onChooseImport={() => void chooseImport()}
                onConfirmImport={() => void confirmImport()}
                onCancelImport={() => setPendingImport(null)}
                onRequestReset={() => setConfirmingReset(true)}
                onConfirmReset={() => void runReset()}
                onCancelReset={() => setConfirmingReset(false)}
              />

              {import.meta.env.DEV ? (
                <DevToolsSection demoBusy={demoBusy} onRunDemo={(scenario) => void runDemo(scenario)} />
              ) : null}
            </>
          )}

          {appVersion ? (
            <footer className="settings-footer">Lumen {appVersion}</footer>
          ) : null}
        </div>
      </div>
    </main>
  );
}
