import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";
// @ts-expect-error type error without @types/node package
import process from "node:process";
// @ts-expect-error type error without @types/node package
import { readFileSync } from "node:fs";
const host = process.env.TAURI_DEV_HOST;
const tauriDebug = !!process.env.TAURI_ENV_DEBUG;
const pkgVersion = (JSON.parse(readFileSync("package.json", "utf8")) as { version: string }).version;

// https://vite.dev/config/
export default defineConfig(() => ({
  plugins: [react()],

  // 版本号的唯一来源是 package.json：编译期注入，浏览器 mock 与 Tauri 运行时
  // 都能拿到同一个值（Tauri 端以 `getVersion()` 为准）。
  define: {
    __APP_VERSION__: JSON.stringify(pkgVersion),
  },

  // Tauri 的 WebView（WebView2 / WKWebView / WebKitGTK）都是常青内核，目标放宽到
  // esnext，避免为老浏览器降级转译、注入多余辅助代码。压缩交给 Vite 默认的 Oxc。
  build: {
    target: "esnext",
    sourcemap: tauriDebug,
  },

  // Vite options tailored for Tauri development and only applied in `tauri dev` or `tauri build`
  //
  // 1. prevent Vite from obscuring rust errors
  clearScreen: false,
  // 2. tauri expects a fixed port, fail if that port is not available
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host
      ? {
          protocol: "ws",
          host,
          port: 1421,
        }
      : undefined,
    watch: {
      // 3. tell Vite to ignore watching `src-tauri`
      ignored: ["**/src-tauri/**"],
    },
  },
}));
