export function isTauriRuntime(value: unknown): boolean {
  return typeof value === "object" && value !== null && "__TAURI_INTERNALS__" in value;
}
