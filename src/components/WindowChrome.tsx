import { useEffect, useState } from "react";
import { getCurrentWindow, type Window } from "@tauri-apps/api/window";
import { isTauriRuntime } from "./tauriRuntime";

function resolveAppWindow(): Window | null {
  if (!isTauriRuntime(window)) return null;
  try {
    return getCurrentWindow();
  } catch {
    return null;
  }
}

export function WindowChrome() {
  const [appWindow] = useState(resolveAppWindow);
  const [isMaximized, setIsMaximized] = useState(false);

  useEffect(() => {
    if (!appWindow) return;

    let unlisten: (() => void) | undefined;

    void appWindow.isMaximized().then(setIsMaximized);
    void appWindow.onResized(() => {
      void appWindow.isMaximized().then(setIsMaximized);
    }).then((cleanup) => {
      unlisten = cleanup;
    });

    return () => unlisten?.();
  }, [appWindow]);

  if (!appWindow) return null;

  return (
    <div className="window-chrome" aria-label="窗口控制">
      <div className="window-drag-region" data-tauri-drag-region aria-hidden="true" />
      <div className="window-controls">
        <button
          className="window-control"
          type="button"
          aria-label="最小化"
          onClick={() => void appWindow.minimize()}
        >
          <span aria-hidden="true" className="window-icon window-icon-minimize" />
        </button>
        <button
          className="window-control"
          type="button"
          aria-label={isMaximized ? "还原" : "最大化"}
          onClick={() => void appWindow.toggleMaximize()}
        >
          <span
            aria-hidden="true"
            className={`window-icon ${isMaximized ? "window-icon-restore" : "window-icon-maximize"}`}
          />
        </button>
        <button
          className="window-control window-control-close"
          type="button"
          aria-label="关闭"
          onClick={() => void appWindow.close()}
        >
          <span aria-hidden="true" className="window-icon window-icon-close" />
        </button>
      </div>
    </div>
  );
}
