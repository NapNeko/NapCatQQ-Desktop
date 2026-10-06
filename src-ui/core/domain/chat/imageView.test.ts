import { describe, expect, it } from 'vitest';
import { imageScale, zoomScroll } from './imageView';

describe('chat image viewport', () => {
    it('opens long screenshots at a readable width instead of squeezing the entire height', () => {
        expect(imageScale({ width: 800, height: 6000 }, { width: 1000, height: 650 }, 'auto')).toBe(
            1,
        );
        expect(
            imageScale({ width: 800, height: 6000 }, { width: 1000, height: 650 }, 'fit'),
        ).toBeCloseTo(602 / 6000);
    });
    it('fits ordinary landscape images without enlarging small images', () => {
        expect(imageScale({ width: 1600, height: 900 }, { width: 848, height: 548 }, 'auto')).toBe(
            0.5,
        );
        expect(imageScale({ width: 100, height: 100 }, { width: 848, height: 548 }, 'auto')).toBe(
            1,
        );
    });
    it('keeps the point under the cursor stable when an image grows beyond the canvas', () => {
        expect(zoomScroll(0, 500, 1000, 800, 1, 2)).toBe(324);
        expect(zoomScroll(324, 500, 1000, 800, 2, 1)).toBe(0);
    });
});
