import { fireEvent, render, screen, waitFor } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import type { SnowlumaQrLoginResult } from '../../../../core/ipc/types';
import { SnowlumaQrCodeDialog } from './SnowlumaQrCodeDialog';

vi.mock('../../../../hooks/theme/useThemeTokens', () => ({
    useThemeTokens: () => ({ foreground: '#1a120d', background: '#ffffff' }),
}));
vi.mock('../../../../hooks/preferences/useMotion', () => ({
    useMotion: () => ({
        enabled: false,
        level: 'elegant',
        speed: 1,
        preset: {
            feel: { hoverScale: 1, tapScale: 1 },
            timing: { ease: { pop: 'none', hover: 'none' } },
        },
        duration: () => 0,
        ease: { enter: 'none', exit: 'none', damped: 'none' },
        bindHover: () => () => {},
        bindPress: () => () => {},
    }),
}));

function session() {
    return {
        serverId: 'server-a',
        botId: '10001',
        sessionId: 'qr-1',
        captureGeneration: 0n,
    };
}

describe('SnowlumaQrCodeDialog', () => {
    it('renders the volatile payload as a local QR', async () => {
        const result: SnowlumaQrLoginResult = {
            status: 'payload',
            session: session(),
            payload: 'https://qq.example/login?token=volatile',
        };

        render(
            <SnowlumaQrCodeDialog
                open
                botId="10001"
                result={result}
                onOpenChange={vi.fn()}
                onOpenNovnc={vi.fn()}
            />,
        );

        await waitFor(() => expect(screen.getByText('SnowLuma 扫码登录 · 10001')).toBeInTheDocument());
        await waitFor(() => expect(screen.getByTestId('snowluma-qr-svg')).toBeInTheDocument());
        expect(screen.queryByText(result.payload)).not.toBeInTheDocument();
    });

    it('offers the existing noVNC action for fallback results', async () => {
        const onOpenNovnc = vi.fn();
        const result: SnowlumaQrLoginResult = {
            status: 'fallback_no_vnc',
            session: session(),
            reason: 'capability_unavailable',
        };

        render(
            <SnowlumaQrCodeDialog
                open
                botId="10001"
                result={result}
                onOpenChange={vi.fn()}
                onOpenNovnc={onOpenNovnc}
            />,
        );

        expect(screen.getByText('二维码提取不可用，请使用 noVNC 完成登录。')).toBeInTheDocument();
        await waitFor(() => expect(screen.getByRole('button', { name: '打开 noVNC 桌面' })).toBeInTheDocument());
        fireEvent.click(screen.getByRole('button', { name: '打开 noVNC 桌面' }));
        expect(onOpenNovnc).toHaveBeenCalledTimes(1);
    });
});
