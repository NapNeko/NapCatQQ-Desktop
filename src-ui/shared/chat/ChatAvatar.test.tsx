// @vitest-environment jsdom
import { act, cleanup, fireEvent, render } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { AVATAR_REFRESH_MS } from '../../hooks/ui/useAvatarUrl';
import { ChatAvatar } from './ChatAvatar';

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

describe('chat avatars', () => {
    it('requests group portraits and retains the fallback when loading fails', () => {
        const { container } = render(
            <ChatAvatar contact={{ type: 'group', id: '20001', name: '开发交流' }} />,
        );
        const image = container.querySelector('img');
        expect(image).toHaveAttribute('src', 'https://p.qlogo.cn/gh/20001/20001/640?_ncdAvatar=10');
        fireEvent.error(image!);
        expect(container.querySelector('img')).toBeNull();
        expect(container.querySelector('svg')).not.toBeNull();

        act(() => vi.advanceTimersByTime(AVATAR_REFRESH_MS));
        expect(container.querySelector('img')).toHaveAttribute(
            'src',
            'https://p.qlogo.cn/gh/20001/20001/640?_ncdAvatar=11',
        );
    });

    it('refreshes a visible private portrait without a parent render', () => {
        const { container } = render(
            <ChatAvatar contact={{ type: 'private', id: '12345', name: '小猫' }} />,
        );
        expect(container.querySelector('img')).toHaveAttribute(
            'src',
            'https://q1.qlogo.cn/g?b=qq&nk=12345&s=100&_ncdAvatar=10',
        );
        act(() => vi.advanceTimersByTime(AVATAR_REFRESH_MS));
        expect(container.querySelector('img')).toHaveAttribute(
            'src',
            'https://q1.qlogo.cn/g?b=qq&nk=12345&s=100&_ncdAvatar=11',
        );
    });

    it('does not carry a failed portrait over when the contact changes', () => {
        const { container, rerender } = render(
            <ChatAvatar contact={{ type: 'private', id: '12345', name: '旧号' }} />,
        );
        fireEvent.error(container.querySelector('img')!);
        rerender(<ChatAvatar contact={{ type: 'private', id: '67890', name: '新号' }} />);
        expect(container.querySelector('img')).toHaveAttribute(
            'src',
            'https://q1.qlogo.cn/g?b=qq&nk=67890&s=100&_ncdAvatar=10',
        );
    });

    it.each([
        { type: 'group' as const, id: '../bad' },
        { type: 'group' as const, id: '0' },
        { type: 'private' as const, id: 'abc' },
        { type: 'private' as const, id: '1234' },
    ])('does not request a portrait for invalid contact $type:$id', (contact) => {
        const { container } = render(<ChatAvatar contact={{ ...contact, name: '未知' }} />);
        expect(container.querySelector('img')).toBeNull();
        expect(vi.getTimerCount()).toBe(0);
    });
});
