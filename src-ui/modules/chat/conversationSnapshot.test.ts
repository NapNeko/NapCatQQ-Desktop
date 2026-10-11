// @vitest-environment jsdom
import { describe, expect, it, vi } from 'vitest';
import { createConversationSnapshot } from './conversationSnapshot';

describe('conversation snapshots', () => {
    it('freezes the previous viewport while the next composer changes its available height', () => {
        const frame = document.createElement('div');
        vi.spyOn(frame, 'clientWidth', 'get').mockReturnValue(680);
        vi.spyOn(frame, 'clientHeight', 'get').mockReturnValue(420);
        const body = document.createElement('div');
        body.innerHTML = '<button id="old-action">复制</button>';
        const frozen = createConversationSnapshot(body, frame);
        expect(frozen.style.width).toBe('680px');
        expect(frozen.style.height).toBe('420px');
        expect(frozen.style.bottom).toBe('auto');
        expect(frozen.inert).toBe(true);
        expect(frozen.getAttribute('aria-hidden')).toBe('true');
        expect(frozen.querySelector('[id]')).toBeNull();
        expect(body.querySelector('[id]')).not.toBeNull();
    });

    it('keeps a visible video frame without duplicating media sources or changing the player', () => {
        const frame = document.createElement('div');
        const body = document.createElement('div');
        body.innerHTML =
            '<video src="video.mp4" autoplay><source src="fallback.mp4"></video><audio src="voice.ogg"></audio>';
        const video = body.querySelector('video')!;
        vi.spyOn(video, 'clientWidth', 'get').mockReturnValue(320);
        vi.spyOn(video, 'clientHeight', 'get').mockReturnValue(180);
        vi.spyOn(video, 'videoWidth', 'get').mockReturnValue(1920);
        vi.spyOn(video, 'videoHeight', 'get').mockReturnValue(1080);
        vi.spyOn(video, 'readyState', 'get').mockReturnValue(2);
        const drawImage = vi.fn();
        vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue({
            fillRect: vi.fn(),
            drawImage,
        } as unknown as CanvasRenderingContext2D);
        const frozen = createConversationSnapshot(body, frame);
        expect(frozen.querySelector('video,audio,source')).toBeNull();
        expect(frozen.querySelector('canvas')?.width).toBe(320);
        expect(frozen.querySelector('canvas')?.style.display).toBe('block');
        expect(drawImage).toHaveBeenCalledWith(video, 0, 0, 320, 180);
        expect(video.getAttribute('src')).toBe('video.mp4');
        expect(video.hasAttribute('autoplay')).toBe(true);
        expect(body.querySelector('audio')?.getAttribute('src')).toBe('voice.ogg');
    });
});
