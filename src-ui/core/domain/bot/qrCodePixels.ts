// 从内存中的截图像素读取二维码，不保留扫码材料。
import jsQR from 'jsqr';

const MAX_PIXELS = 4 * 1024 * 1024;
const MAX_PAYLOAD_LENGTH = 8192;

export function decodeQrPixels(
    data: Uint8ClampedArray,
    width: number,
    height: number,
): string | null {
    if (
        !Number.isSafeInteger(width) ||
        !Number.isSafeInteger(height) ||
        width <= 0 ||
        height <= 0
    ) {
        return null;
    }
    const pixels = width * height;
    if (pixels > MAX_PIXELS || data.length !== pixels * 4) return null;

    try {
        const payload = jsQR(data, width, height, { inversionAttempts: 'attemptBoth' })?.data;
        return payload && payload.length <= MAX_PAYLOAD_LENGTH ? payload : null;
    } catch {
        return null;
    }
}
