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

// Radix 的指针交互与 CodeMirror 的文字测量会调用这些 jsdom 缺失的 API。
Element.prototype.hasPointerCapture ??= () => false;
Element.prototype.releasePointerCapture ??= () => {};
Element.prototype.setPointerCapture ??= () => {};
Element.prototype.scrollIntoView ??= () => {};
Range.prototype.getClientRects ??= () =>
    ({
        length: 0,
        item: () => null,
        [Symbol.iterator]: [][Symbol.iterator],
    }) as unknown as DOMRectList;
Range.prototype.getBoundingClientRect ??= () => new DOMRect();

// 限制动画到浏览器帧率，并在环境销毁时清掉 Node 定时器。
const pendingFrames = new Set<number>();
let windowClosed = false;

const raf = vi.fn((callback: FrameRequestCallback) => {
    if (windowClosed) return 0;
    const handle = window.setTimeout(() => {
        pendingFrames.delete(handle);
        callback(performance.now());
    }, 16);
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
