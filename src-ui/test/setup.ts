import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

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

const raf = vi.fn((callback: FrameRequestCallback) => window.setTimeout(() => callback(performance.now()), 0));
const caf = vi.fn((handle: number) => window.clearTimeout(handle));

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
