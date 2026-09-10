import { invoke } from "@tauri-apps/api/core";
import { isTauriRuntime } from "../components/tauriRuntime";

export type Settings = {
  port: number;
  closeToTray: boolean;
};

let mockSettings: Settings = { port: 8787, closeToTray: true };

export async function getSettings(): Promise<Settings> {
  if (!isTauriRuntime(window)) return { ...mockSettings };
  return invoke<Settings>("get_settings_cmd");
}

export async function saveSettings(input: Settings): Promise<Settings> {
  if (!isTauriRuntime(window)) {
    mockSettings = { ...input };
    return { ...mockSettings };
  }
  return invoke<Settings>("save_settings_cmd", { input });
}

export async function getAutostart(): Promise<boolean> {
  if (!isTauriRuntime(window)) return false;
  return invoke<boolean>("get_autostart_cmd");
}

export async function setAutostart(enabled: boolean): Promise<boolean> {
  if (!isTauriRuntime(window)) return enabled;
  return invoke<boolean>("set_autostart_cmd", { enabled });
}

export async function exportSeed(): Promise<string> {
  if (!isTauriRuntime(window)) return "lumen.seed.json";
  return invoke<string>("export_seed_cmd");
}

export async function resetData(): Promise<void> {
  if (!isTauriRuntime(window)) return;
  return invoke<void>("reset_data_cmd");
}
