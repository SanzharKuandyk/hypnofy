import { getCurrentWindow } from "@tauri-apps/api/window";

export async function guardClose(
  flush: () => Promise<void>,
  failed: () => void,
): Promise<() => void> {
  const window = getCurrentWindow();
  let closing = false;
  return window.onCloseRequested(async (event) => {
    if (closing) return;
    event.preventDefault();
    try {
      await flush();
      closing = true;
      await window.close();
    } catch {
      failed();
    }
  });
}
