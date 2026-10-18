import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import gsap from 'gsap';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createDefaultConnection } from '../../../../core/domain/bot/connections';
import { ConnectionEditor } from './ConnectionEditor';

function finishMotion() {
    act(() => {
        gsap.globalTimeline.getChildren().forEach((animation) => animation.totalProgress(1));
    });
}

afterEach(cleanup);

describe('连接编辑器底座字段切换', () => {
    it('切到 SnowLuma 时保留退出内容，并立即禁止隐藏字段参与交互', () => {
        const initialData = createDefaultConnection('httpServer');
        const props = {
            kind: 'httpServer' as const,
            initialData,
            existingNames: [],
            onSave: vi.fn(),
            onCancel: vi.fn(),
        };
        const { rerender } = render(<ConnectionEditor {...props} backendType="napcat" />);
        finishMotion();
        const cors = screen.getByRole('checkbox', { name: '允许跨域请求 (CORS)' });

        rerender(<ConnectionEditor {...props} backendType="snowluma" />);

        expect(cors).toBeInTheDocument();
        expect(cors.closest('[inert]')).not.toBeNull();
        expect(cors.closest('[aria-hidden="true"]')).not.toBeNull();
        finishMotion();
        expect(screen.queryByText('允许跨域请求 (CORS)')).not.toBeInTheDocument();
    });

    it('快速切走再切回 SnowLuma 时保留已编辑的超时值', () => {
        const initialData = createDefaultConnection('httpClient');
        const props = {
            kind: 'httpClient' as const,
            initialData,
            existingNames: [],
            onSave: vi.fn(),
            onCancel: vi.fn(),
        };
        const { rerender } = render(<ConnectionEditor {...props} backendType="snowluma" />);
        finishMotion();
        fireEvent.change(screen.getByLabelText('请求超时 (ms)'), { target: { value: '8500' } });

        rerender(<ConnectionEditor {...props} backendType="napcat" />);
        rerender(<ConnectionEditor {...props} backendType="snowluma" />);
        finishMotion();

        const timeout = screen.getByLabelText('请求超时 (ms)');
        expect(timeout).toHaveValue(8500);
        expect(timeout.closest('[inert]')).toBeNull();
    });
});
