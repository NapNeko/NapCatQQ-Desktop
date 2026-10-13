// 截图只用于本地解码；弹窗重新绘制编码内容，避免 QQ 的缩放与覆盖层影响扫码。
import { decodeQrPixels } from '../../../../core/domain/bot/qrCodePixels';

const PNG_PREFIX = 'data:image/png;base64,';
const MAX_IMAGE_PIXELS = 4 * 1024 * 1024;

function loadCapture(payload: string, signal: AbortSignal): Promise<HTMLImageElement> {
    return new Promise((resolve, reject) => {
        const image = new Image();
        const finish = (error?: Error) => {
            clearTimeout(timer);
            signal.removeEventListener('abort', abort);
            image.onload = null;
            image.onerror = null;
            if (error) {
                image.src = '';
                reject(error);
            } else resolve(image);
        };
        const abort = () => finish(new DOMException('Aborted', 'AbortError'));
        const timer = setTimeout(() => finish(new Error('QR image timeout')), 5000);
        image.onload = () => finish();
        image.onerror = () => finish(new Error('Invalid QR image'));
        signal.addEventListener('abort', abort, { once: true });
        if (signal.aborted) abort();
        else image.src = payload;
    });
}

export async function resolveLoginQrPayload(payload: string, signal: AbortSignal): Promise<string> {
    if (signal.aborted) throw new DOMException('Aborted', 'AbortError');
    if (!payload.startsWith('data:image/')) return payload;
    if (!payload.startsWith(PNG_PREFIX) || payload.length > 128 * 1024)
        throw new Error('Invalid QR capture');
    const image = await loadCapture(payload, signal);
    const canvas = document.createElement('canvas');
    try {
        const width = image.naturalWidth;
        const height = image.naturalHeight;
        if (!width || !height || width * height > MAX_IMAGE_PIXELS)
            throw new Error('Invalid QR dimensions');
        const context = canvas.getContext('2d', { willReadFrequently: true });
        if (!context) throw new Error('QR canvas unavailable');
        for (const scale of [1, 2]) {
            if (signal.aborted) throw new DOMException('Aborted', 'AbortError');
            const padding = 16 * scale;
            const scaledWidth = width * scale + padding * 2;
            const scaledHeight = height * scale + padding * 2;
            if (scaledWidth * scaledHeight > MAX_IMAGE_PIXELS) break;
            canvas.width = scaledWidth;
            canvas.height = scaledHeight;
            context.fillStyle = '#ffffff';
            context.fillRect(0, 0, canvas.width, canvas.height);
            context.imageSmoothingEnabled = false;
            context.drawImage(image, padding, padding, width * scale, height * scale);
            const pixels = context.getImageData(0, 0, canvas.width, canvas.height);
            try {
                const content = decodeQrPixels(pixels.data, pixels.width, pixels.height);
                if (content) return content;
            } finally {
                pixels.data.fill(0);
            }
        }
        throw new Error('QR capture could not be decoded');
    } finally {
        image.src = '';
        canvas.width = 0;
        canvas.height = 0;
    }
}
