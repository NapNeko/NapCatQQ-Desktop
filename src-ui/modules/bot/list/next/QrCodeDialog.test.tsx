import { act, cleanup, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { QrCodeDialog } from './QrCodeDialog';

const qr = vi.hoisted(() => ({ render: vi.fn() }));
vi.mock('qrcode', () => ({ default: { toString: qr.render } }));
vi.mock('../../../../hooks/theme/useThemeTokens', () => ({
    useThemeTokens: () => ({ foreground: '#1a120d', background: '#ffffff' }),
}));

afterEach(() => {
    cleanup();
    qr.render.mockReset();
});

describe('NapCat 二维码更新', () => {
    it.each(['ready', 'failed'] as const)(
        '新二维码生成期间不展示上一份 %s 渲染结果',
        async (previous) => {
            if (previous === 'ready')
                qr.render.mockResolvedValueOnce('<svg><title>旧二维码</title></svg>');
            else qr.render.mockRejectedValueOnce(new Error('旧二维码损坏'));
            let finish!: (svg: string) => void;
            qr.render.mockImplementationOnce(
                () =>
                    new Promise<string>((resolve) => {
                        finish = resolve;
                    }),
            );
            const props = { open: true, onOpenChange: vi.fn(), botId: '12345', isOnline: false };
            const { rerender } = render(
                <QrCodeDialog {...props} qrcodeUrl="https://example.com/qr/old" />,
            );
            await act(async () => {});
            if (previous === 'ready') expect(screen.getByTitle('旧二维码')).toBeInTheDocument();
            else expect(screen.getByText('旧二维码损坏')).toBeInTheDocument();

            rerender(<QrCodeDialog {...props} qrcodeUrl="https://example.com/qr/new" />);
            expect(screen.queryByTitle('旧二维码')).toBeNull();
            expect(screen.queryByText('旧二维码损坏')).toBeNull();
            expect(screen.getByText('等待二维码…')).toBeInTheDocument();
            await act(async () => finish('<svg><title>新二维码</title></svg>'));
            expect(screen.getByTitle('新二维码')).toBeInTheDocument();
        },
    );
});
