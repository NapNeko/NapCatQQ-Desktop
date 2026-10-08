import { fireEvent, render } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { ChatAvatar } from './ChatAvatar';

describe('chat avatars', () => {
    it('requests group portraits and retains the fallback when loading fails', () => {
        const { container } = render(
            <ChatAvatar contact={{ type: 'group', id: '20001', name: '开发交流' }} />,
        );
        const image = container.querySelector('img');
        expect(image).toHaveAttribute('src', 'https://p.qlogo.cn/gh/20001/20001/640');
        fireEvent.error(image!);
        expect(container.querySelector('img')).toBeNull();
        expect(container.querySelector('svg')).not.toBeNull();
    });
    it('does not request a portrait for a malformed group identifier', () => {
        const { container } = render(
            <ChatAvatar contact={{ type: 'group', id: '../bad', name: '未知群' }} />,
        );
        expect(container.querySelector('img')).toBeNull();
    });
});
