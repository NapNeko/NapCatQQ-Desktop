// @vitest-environment jsdom
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AVATAR_REFRESH_MS } from '../../hooks/ui/useAvatarUrl';
import { BotAvatar } from './BotAvatar';

beforeEach(() => {
    vi.useFakeTimers();
    vi.setSystemTime(AVATAR_REFRESH_MS * 10);
    vi.spyOn(document, 'visibilityState', 'get').mockReturnValue('visible');
});

afterEach(() => {
    cleanup();
    vi.useRealTimers();
    vi.restoreAllMocks();
});

describe('BotAvatar', () => {
    it('retries a failed portrait automatically in the next cache period', () => {
        const { container } = render(
            <BotAvatar qqid="12345" displayName="小猫" flavorTone="brand" />,
        );
        const image = container.querySelector('img')!;
        const initialUrl = image.getAttribute('src');
        fireEvent.error(image);
        expect(container.querySelector('img')).toBeNull();
        expect(container).toHaveTextContent('小');

        act(() => vi.advanceTimersByTime(AVATAR_REFRESH_MS - 1));
        expect(container.querySelector('img')).toBeNull();
        act(() => vi.advanceTimersByTime(1));
        expect(container.querySelector('img')).toHaveAttribute(
            'src',
            'https://q.qlogo.cn/headimg_dl?dst_uin=12345&spec=640&_ncdAvatar=11',
        );
        expect(container.querySelector('img')?.getAttribute('src')).not.toBe(initialUrl);
    });

    it('does not carry a failed portrait over to another QQ account', () => {
        const { container, rerender } = render(
            <BotAvatar qqid="12345" displayName="旧号" flavorTone="brand" />,
        );
        fireEvent.error(container.querySelector('img')!);
        rerender(<BotAvatar qqid="67890" displayName="新号" flavorTone="brand" />);
        expect(container.querySelector('img')).toHaveAttribute(
            'src',
            'https://q.qlogo.cn/headimg_dl?dst_uin=67890&spec=640&_ncdAvatar=10',
        );
    });

    it.each(['', '../bad', '00000', '1234'])(
        'does not request a portrait for invalid QQ ID %j',
        (qqid) => {
            const { container } = render(
                <BotAvatar qqid={qqid} displayName="小猫" flavorTone="brand" />,
            );
            expect(container.querySelector('img')).toBeNull();
            expect(container).toHaveTextContent('小');
            expect(vi.getTimerCount()).toBe(0);
        },
    );
});
