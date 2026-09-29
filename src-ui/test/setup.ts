import '@testing-library/jest-dom/vitest';
import { afterAll, vi } from 'vitest';

class ResizeObserverMock {
  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
}

class IntersectionObserverMock {
  readonly root = null;
  readonly rootMargin = '';
  readonly thresholds: ReadonlyArray<number> = [];

  observe(): void {}
  unobserve(): void {}
  disconnect(): void {}
  takeRecords(): IntersectionObserverEntry[] {
    return [];
  }
}

Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: vi.fn().mockImplementation((query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: vi.fn(),
    removeListener: vi.fn(),
    addEventListener: vi.fn(),
    removeEventListener: vi.fn(),
    dispatchEvent: vi.fn(),
  })),
});

Object.defineProperty(window, 'ResizeObserver', {
  writable: true,
  value: ResizeObserverMock,
});

Object.defineProperty(window, 'IntersectionObserver', {
  writable: true,
  value: IntersectionObserverMock,
});

globalThis.ResizeObserver = ResizeObserverMock;
globalThis.IntersectionObserver = IntersectionObserverMock as typeof IntersectionObserver;

// 帧走 setTimeout，而 vitest 里 window.setTimeout 就是 Node 的定时器，jsdom 拆环境时不会替它清。
// GSAP 一加载 ticker 就醒，没有动画也要跑到第 30 帧才睡；文件跑得快，拆环境时还挂着一帧，
// 到点后回调里要下一帧，window 已经没了。所以没到的帧自己记着，文件跑完全撤掉、之后也不再排，
// 和窗口关了就不再出帧一样
const pendingFrames = new Set<number>();
let windowClosed = false;

const raf = vi.fn((callback: FrameRequestCallback) => {
  if (windowClosed) return 0;
  const handle = window.setTimeout(() => {
    pendingFrames.delete(handle);
    callback(performance.now());
  }, 0);
  pendingFrames.add(handle);
  return handle;
});
const caf = vi.fn((handle: number) => {
  if (!pendingFrames.delete(handle)) return;
  window.clearTimeout(handle);
});

afterAll(() => {
  windowClosed = true;
  pendingFrames.forEach((handle) => window.clearTimeout(handle));
  pendingFrames.clear();
});

Object.defineProperty(window, 'requestAnimationFrame', {
  writable: true,
  value: raf,
});

Object.defineProperty(window, 'cancelAnimationFrame', {
  writable: true,
  value: caf,
});

globalThis.requestAnimationFrame = raf;
globalThis.cancelAnimationFrame = caf;

Object.defineProperty(window, '__TAURI__', {
  writable: true,
  value: {
    core: {
      invoke: vi.fn(),
    },
    event: {
      listen: vi.fn(),
      emit: vi.fn(),
    },
    path: {},
  },
});
