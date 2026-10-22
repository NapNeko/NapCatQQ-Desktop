/** Coalesce content resizing without recalculating the pinned layout on every notification. */
export function observeLayoutRefresh(target: Element | null, refresh: () => void) {
  let timer: number | undefined;
  let disposed = false;
  let width = -1;
  let height = -1;
  const request = () => {
    if (disposed) return;
    window.clearTimeout(timer);
    timer = window.setTimeout(() => {
      timer = undefined;
      if (!disposed) refresh();
    }, 160);
  };
  const observer = new ResizeObserver(entries => {
    const bounds = entries.find(entry => entry.target === target)?.contentRect;
    if (!bounds || (Math.abs(bounds.width - width) < 1 && Math.abs(bounds.height - height) < 1)) return;
    width = bounds.width;
    height = bounds.height;
    request();
  });
  if (target) observer.observe(target);
  return {
    request,
    dispose() {
      disposed = true;
      observer.disconnect();
      window.clearTimeout(timer);
    },
  };
}
