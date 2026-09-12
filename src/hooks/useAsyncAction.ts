import { useCallback, useState } from "react";

/**
 * 统一异步操作的 busy / 错误归属：开始时清空错误、置忙，失败时把错误写回
 * 调用方指定的槽位，结束时复位 busy。`onError` 用于需要回滚的调用点。
 */
export function useAsyncAction() {
  const [busy, setBusy] = useState(false);

  const run = useCallback(
    async <T,>(
      task: () => Promise<T>,
      setError: (message: string | null) => void,
      onError?: (error: unknown) => void,
    ): Promise<T | undefined> => {
      setBusy(true);
      setError(null);
      try {
        return await task();
      } catch (error) {
        onError?.(error);
        setError(String(error));
        return undefined;
      } finally {
        setBusy(false);
      }
    },
    [],
  );

  return { busy, run };
}
