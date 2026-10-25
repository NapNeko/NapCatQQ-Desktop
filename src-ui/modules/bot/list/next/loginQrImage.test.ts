// @vitest-environment jsdom
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { resolveLoginQrPayload } from './loginQrImage';

const decode = vi.hoisted(() => vi.fn());
vi.mock('../../../../core/domain/bot/qrCodePixels', () => ({ decodeQrPixels: decode }));

const CAPTURE = 'data:image/png;base64,c3ludGhldGlj';
const CONTENT = 'https://example.test/synthetic-qr';
let loadedImage: HTMLImageElement;
let pixels: Uint8ClampedArray[];
let context: Pick<CanvasRenderingContext2D, 'fillRect' | 'drawImage' | 'getImageData'>;

beforeEach(() => {
    decode.mockReset().mockReturnValue(null);
    loadedImage = document.createElement('img');
    Object.defineProperties(loadedImage, {
        naturalWidth: { value: 196, configurable: true },
        naturalHeight: { value: 196, configurable: true },
    });
    vi.stubGlobal('Image', function () {
        return loadedImage;
    });
    pixels = [];
    context = {
        fillRect: vi.fn(),
        drawImage: vi.fn(),
        getImageData: vi.fn((_x: number, _y: number, width: number, height: number) => {
            const data = new Uint8ClampedArray(width * height * 4).fill(255);
            pixels.push(data);
            return { data, width, height, colorSpace: 'srgb' } as ImageData;
        }),
    };
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(
        context as CanvasRenderingContext2D,
    );
});

afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    vi.useRealTimers();
});

function load(signal = new AbortController().signal) {
    const result = resolveLoginQrPayload(CAPTURE, signal);
    loadedImage.dispatchEvent(new Event('load'));
    return result;
}

describe('SL QR image material', () => {
    it('keeps a valid capture when both decode attempts fail', async () => {
        await expect(load()).resolves.toEqual({ kind: 'capture', src: CAPTURE });
        expect(decode).toHaveBeenCalledTimes(2);
        expect(pixels.every((data) => data.every((value) => value === 0))).toBe(true);
        const canvas = vi.mocked(HTMLCanvasElement.prototype.getContext).mock
            .contexts[0] as HTMLCanvasElement;
        expect(canvas.width).toBe(0);
        expect(canvas.height).toBe(0);
        expect(loadedImage.getAttribute('src')).toBe('');
    });

    it('returns decoded content together with its validated fallback', async () => {
        decode.mockReturnValue(CONTENT);
        await expect(load()).resolves.toEqual({
            kind: 'content',
            content: CONTENT,
            capture: CAPTURE,
        });
    });

    it.each(['no canvas', 'canvas failure', 'decoder failure'])(
        'can display the capture despite %s',
        async (failure) => {
            if (failure === 'no canvas')
                vi.mocked(HTMLCanvasElement.prototype.getContext).mockReturnValue(null);
            if (failure === 'canvas failure')
                vi.mocked(context.getImageData).mockImplementation(() => {
                    throw new Error('canvas unavailable');
                });
            if (failure === 'decoder failure')
                decode.mockImplementation(() => {
                    throw new Error('decode unavailable');
                });
            await expect(load()).resolves.toEqual({ kind: 'capture', src: CAPTURE });
        },
    );

    it.each([
        [0, 196],
        [196, 0],
        [2049, 2048],
    ])('rejects invalid dimensions %s x %s', async (width, height) => {
        Object.defineProperties(loadedImage, {
            naturalWidth: { value: width },
            naturalHeight: { value: height },
        });
        await expect(load()).rejects.toThrow('Invalid QR dimensions');
        expect(decode).not.toHaveBeenCalled();
    });

    it.each([
        'data:image/svg+xml,<svg/>',
        'data:image/jpeg;base64,c3ludGhldGlj',
        `data:image/png;base64,${'x'.repeat(128 * 1024)}`,
    ])('rejects unsupported or oversized captures', async (payload) => {
        await expect(resolveLoginQrPayload(payload, new AbortController().signal)).rejects.toThrow(
            'Invalid QR capture',
        );
        expect(loadedImage.getAttribute('src')).toBeNull();
    });

    it('does not fall back to an image that the browser cannot load', async () => {
        const result = resolveLoginQrPayload(CAPTURE, new AbortController().signal);
        loadedImage.dispatchEvent(new Event('error'));
        await expect(result).rejects.toThrow('Invalid QR image');
        expect(decode).not.toHaveBeenCalled();
    });

    it('bounds image loading and clears timed-out material', async () => {
        vi.useFakeTimers();
        const result = resolveLoginQrPayload(CAPTURE, new AbortController().signal);
        const assertion = expect(result).rejects.toThrow('QR image timeout');
        await vi.advanceTimersByTimeAsync(5000);
        await assertion;
        expect(loadedImage.getAttribute('src')).toBe('');
    });

    it.each(['before load', 'after load'])(
        'does not return a capture after cancellation %s',
        async (when) => {
            const controller = new AbortController();
            const result = resolveLoginQrPayload(CAPTURE, controller.signal);
            if (when === 'after load') loadedImage.dispatchEvent(new Event('load'));
            controller.abort();
            await expect(result).rejects.toMatchObject({ name: 'AbortError' });
            expect(loadedImage.getAttribute('src')).toBe('');
        },
    );

    it('keeps text payloads available for QR generation', async () => {
        await expect(resolveLoginQrPayload(CONTENT, new AbortController().signal)).resolves.toEqual(
            { kind: 'content', content: CONTENT },
        );
    });
});
