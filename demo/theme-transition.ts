// 缩放 iframe 不使用整页 View Transition 快照，避免 Chromium 合成层白屏。
interface ThemeTransitionOptions {
  enabled: boolean;
  level: 'elegant' | 'standard' | 'rich';
  duration: number;
  easing: string;
  originX?: number;
  originY?: number;
}

let current: Animation | undefined;

function cancel() {
  current?.cancel();
  current = undefined;
}

document.addEventListener('visibilitychange', () => { if (document.hidden) cancel(); });
window.addEventListener('pagehide', cancel);

export async function playThemeTransition(changeTheme: () => void, options: ThemeTransitionOptions): Promise<void> {
  cancel();
  changeTheme();
  const surface = document.querySelector<HTMLElement>('.demo-desktop');
  if (!surface || !options.enabled || options.duration <= 0 || document.hidden
    || matchMedia('(prefers-reduced-motion: reduce)').matches) return;

  try {
    const animation = surface.animate([{ opacity: .92 }, { opacity: 1 }], {
      duration: Math.min(280, options.duration * 1000), easing: 'ease-out',
    });
    current = animation;
    // 主题保存立即完成；隐藏、重置或再次切换时，不等旧动画结束。
    void animation.finished.catch(() => undefined).then(() => {
      if (current === animation) current = undefined;
    });
  } catch {
    // 不支持局部动画时保留已经应用的主题。
  }
}
