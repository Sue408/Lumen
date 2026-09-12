/**
 * 桌面壳里要的是应用自己的交互，而不是 WebView 的原生右键菜单。
 * 全局吞掉 contextmenu，但在可编辑控件上放行，保留粘贴 / 全选等编辑操作。
 * 返回卸载函数，便于测试与热重载。
 */
export function disableNativeContextMenu(): () => void {
  const onContextMenu = (event: MouseEvent) => {
    const target = event.target as HTMLElement | null;
    if (target?.closest("input, textarea, [contenteditable='true']")) return;
    event.preventDefault();
  };

  document.addEventListener("contextmenu", onContextMenu);
  return () => document.removeEventListener("contextmenu", onContextMenu);
}
