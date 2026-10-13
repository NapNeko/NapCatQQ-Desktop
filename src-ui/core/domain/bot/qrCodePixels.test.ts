import QRCode from 'qrcode';
import { describe, expect, it } from 'vitest';
import { decodeQrPixels } from './qrCodePixels';

const PAYLOAD = 'https://txz.qq.com/p?k=synthetic-login-code&from=desktop';

function qrImage(payload: string, { inverted = false, logo = false } = {}) {
    const { modules } = QRCode.create(payload, { errorCorrectionLevel: 'H' });
    const scale = 5;
    const quietZone = 4;
    const insetX = 17;
    const insetY = 29;
    const qrSize = (modules.size + quietZone * 2) * scale;
    const width = qrSize + insetX * 2;
    const height = qrSize + insetY * 2;
    const data = new Uint8ClampedArray(width * height * 4);
    const background = inverted ? 19 : 236;
    for (let index = 0; index < data.length; index += 4) {
        data[index] = data[index + 1] = data[index + 2] = background;
        data[index + 3] = 255;
    }
    for (let y = 0; y < qrSize; y += 1) {
        for (let x = 0; x < qrSize; x += 1) {
            const row = Math.floor(y / scale) - quietZone;
            const column = Math.floor(x / scale) - quietZone;
            const dark =
                row >= 0 &&
                row < modules.size &&
                column >= 0 &&
                column < modules.size &&
                modules.get(row, column) === 1;
            const value = dark !== inverted ? 12 : 255;
            const index = ((y + insetY) * width + x + insetX) * 4;
            data[index] = data[index + 1] = data[index + 2] = value;
        }
    }
    if (logo) {
        const logoSize = scale * 5;
        const left = insetX + Math.floor((qrSize - logoSize) / 2);
        const top = insetY + Math.floor((qrSize - logoSize) / 2);
        for (let y = 0; y < logoSize; y += 1) {
            for (let x = 0; x < logoSize; x += 1) {
                const index = ((y + top) * width + x + left) * 4;
                const inside =
                    x >= scale && x < logoSize - scale && y >= scale && y < logoSize - scale;
                data[index] = inside ? 30 : 255;
                data[index + 1] = inside ? 100 : 255;
                data[index + 2] = inside ? 210 : 255;
            }
        }
    }
    return { data, width, height };
}

describe('QR screenshot pixels', () => {
    it.each([false, true])('reads the original payload with inverted=%s', (inverted) => {
        const { data, width, height } = qrImage(PAYLOAD, { inverted });
        expect(decodeQrPixels(data, width, height)).toBe(PAYLOAD);
    });

    it('reads a screenshot with a quiet zone and a small central logo', () => {
        const { data, width, height } = qrImage(PAYLOAD, { logo: true });
        expect(decodeQrPixels(data, width, height)).toBe(PAYLOAD);
    });

    it('preserves Unicode and literal URL characters', () => {
        const payload = 'https://example.test/扫码?k=a%2Bb&next=%2Flogin#确认';
        const { data, width, height } = qrImage(payload);
        expect(decodeQrPixels(data, width, height)).toBe(payload);
    });

    it('returns no payload for an empty image', () => {
        expect(decodeQrPixels(new Uint8ClampedArray(64 * 64 * 4).fill(255), 64, 64)).toBeNull();
    });

    it.each([
        [0, 1],
        [1, 0],
        [-1, 1],
        [1, -1],
        [1.5, 1],
        [1, 1.5],
        [NaN, 1],
        [1, Infinity],
        [Number.MAX_SAFE_INTEGER + 1, 1],
    ])('rejects invalid dimensions %s x %s', (width, height) => {
        expect(decodeQrPixels(new Uint8ClampedArray(4), width, height)).toBeNull();
    });

    it('requires exactly four pixel channels per image position', () => {
        expect(decodeQrPixels(new Uint8ClampedArray(3), 1, 1)).toBeNull();
        expect(decodeQrPixels(new Uint8ClampedArray(8), 1, 1)).toBeNull();
    });

    it('rejects an image exceeding the pixel budget even when it contains a QR code', () => {
        const image = qrImage(PAYLOAD);
        const width = 2049;
        const height = 2048;
        const data = new Uint8ClampedArray(width * height * 4).fill(255);
        for (let y = 0; y < image.height; y += 1) {
            data.set(
                image.data.subarray(y * image.width * 4, (y + 1) * image.width * 4),
                (y + 32) * width * 4 + 32 * 4,
            );
        }
        expect(decodeQrPixels(data, width, height)).toBeNull();
    });
});
