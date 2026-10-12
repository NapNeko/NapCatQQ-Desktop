// @vitest-environment jsdom
import { act, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { RemoteSnowLumaQrLoginDialog } from './RemoteSnowLumaQrLoginDialog';
import type { RemoteQrLoginApi } from '../../../../hooks/webui/useRemoteSnowlumaQrLogin';
import type { SnowLumaQrLoginSnapshot } from '../../../../core/ipc/generated/domain/SnowLumaQrLoginSnapshot';

const material: SnowLumaQrLoginSnapshot = {
    v: 1,
    state: 'waiting_for_qr',
    qq_pid: 19,
    qrcode_payload: 'https://txz.qq.com/synthetic',
    uin: null,
    observer_removed: false,
    message: null,
};

describe('SL QR dialog status', () => {
    afterEach(() => {
        vi.useRealTimers();
    });

    it.each([
        ['waiting_for_qr', '正在等待 QQ 生成二维码'],
        ['starting', '正在等待 QQ 登录窗口'],
    ] as const)(
        'keeps %s stable while an ordinary status poll is pending',
        async (state, label) => {
            vi.useFakeTimers();
            const qrLoginSnapshot = vi
                .fn()
                .mockResolvedValueOnce({ ...material, state, qrcode_payload: null })
                .mockImplementation(() => new Promise<SnowLumaQrLoginSnapshot>(() => {}));
            const api: RemoteQrLoginApi = { qrLoginSnapshot, closeQrLogin: vi.fn(async () => {}) };
            const { unmount } = render(
                <RemoteSnowLumaQrLoginDialog
                    botId="123456"
                    open
                    isOnline={false}
                    onOpenChange={vi.fn()}
                    api={api}
                />,
            );
            await act(async () => {});
            expect(screen.getByRole('status').textContent).toBe(label);
            await act(async () => {
                await vi.advanceTimersByTimeAsync(1000);
            });
            expect(qrLoginSnapshot).toHaveBeenCalledTimes(2);
            expect(screen.getByRole('status').textContent).toBe(label);
            unmount();
        },
    );

    it('shows acquisition only on the first request or an explicit refresh', async () => {
        let resolveFirst!: (value: SnowLumaQrLoginSnapshot) => void;
        const qrLoginSnapshot = vi
            .fn()
            .mockImplementationOnce(
                () =>
                    new Promise<SnowLumaQrLoginSnapshot>((resolve) => {
                        resolveFirst = resolve;
                    }),
            )
            .mockImplementation(() => new Promise<SnowLumaQrLoginSnapshot>(() => {}));
        const api: RemoteQrLoginApi = { qrLoginSnapshot, closeQrLogin: vi.fn(async () => {}) };
        const { unmount } = render(
            <RemoteSnowLumaQrLoginDialog
                botId="123456"
                open
                isOnline={false}
                onOpenChange={vi.fn()}
                api={api}
            />,
        );
        await waitFor(() => expect(screen.getByRole('status').textContent).toBe('正在获取二维码'));
        await act(async () => {
            resolveFirst({ ...material, qrcode_payload: null });
        });
        expect(screen.getByRole('status').textContent).toBe('正在等待 QQ 生成二维码');
        fireEvent.click(screen.getByRole('button', { name: '刷新二维码' }));
        await waitFor(() => expect(qrLoginSnapshot).toHaveBeenLastCalledWith('123456', true));
        expect(screen.getByRole('status').textContent).toBe('正在获取二维码');
        unmount();
    });

    it('keeps a valid QR visible while a status poll is pending', async () => {
        const qrLoginSnapshot = vi
            .fn()
            .mockResolvedValueOnce({ ...material, qrcode_payload: 'data:image/png;base64,YQ==' })
            .mockImplementation(() => new Promise<SnowLumaQrLoginSnapshot>(() => {}));
        const api: RemoteQrLoginApi = { qrLoginSnapshot, closeQrLogin: vi.fn(async () => {}) };
        const { unmount } = render(
            <RemoteSnowLumaQrLoginDialog
                botId="123456"
                open
                isOnline={false}
                onOpenChange={vi.fn()}
                api={api}
            />,
        );
        await waitFor(() =>
            expect(screen.getByRole('img', { name: 'QQ 登录二维码' })).toBeTruthy(),
        );
        await waitFor(() => expect(qrLoginSnapshot).toHaveBeenCalledTimes(2), { timeout: 2500 });
        expect(screen.getByRole('img', { name: 'QQ 登录二维码' })).toBeTruthy();
        expect(screen.getByRole('status').textContent).toBe('请用手机 QQ 扫码');
        unmount();
    });

    it.each([
        ['scanned', '已扫码，请在手机上确认'],
        ['login_detected', '正在确认 SL 登录状态'],
        ['logged_in', '登录成功，二维码已移除'],
        ['cleanup_failed', '扫码通道关闭失败，请停止此 Bot'],
    ] as const)(
        'does not display QR or a stale unavailable message in %s',
        async (state, label) => {
            const api: RemoteQrLoginApi = {
                qrLoginSnapshot: vi.fn(async () => ({
                    ...material,
                    state,
                    message: '二维码不可用',
                })),
                closeQrLogin: vi.fn(async () => {}),
            };
            const { unmount } = render(
                <RemoteSnowLumaQrLoginDialog
                    botId="123456"
                    open
                    isOnline={false}
                    onOpenChange={vi.fn()}
                    api={api}
                />,
            );
            await waitFor(() => expect(screen.getByRole('status').textContent).toBe(label));
            expect(screen.queryByRole('img', { name: 'QQ 登录二维码' })).toBeNull();
            if (state === 'logged_in' || state === 'cleanup_failed') {
                expect(screen.queryByRole('button', { name: '刷新二维码' })).toBeNull();
                expect(
                    state === 'logged_in'
                        ? screen.getByRole('button', { name: '完成' })
                        : screen.getByText('关闭', { selector: 'button' }),
                ).toBeTruthy();
            } else {
                expect(
                    (screen.getByRole('button', { name: '刷新二维码' }) as HTMLButtonElement)
                        .disabled,
                ).toBe(true);
            }
            unmount();
        },
    );

    it('allows manual refresh of an expired QR and preserves the QQ fallback', async () => {
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => ({ ...material, state: 'expired' as const })),
            closeQrLogin: vi.fn(async () => {}),
        };
        const { unmount } = render(
            <RemoteSnowLumaQrLoginDialog
                botId="123456"
                open
                isOnline={false}
                onOpenChange={vi.fn()}
                onOpenVnc={vi.fn()}
                api={api}
            />,
        );
        await waitFor(() =>
            expect(screen.getByRole('status').textContent).toBe('二维码已失效，请刷新'),
        );
        expect(screen.queryByRole('img', { name: 'QQ 登录二维码' })).toBeNull();
        expect(
            (screen.getByRole('button', { name: '刷新二维码' }) as HTMLButtonElement).disabled,
        ).toBe(false);
        expect(screen.getByRole('button', { name: '打开 QQ 界面' })).toBeTruthy();
        unmount();
    });

    it('does not start a snapshot request after the Bot is already online', async () => {
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => material),
            closeQrLogin: vi.fn(async () => {}),
        };
        const onOpenChange = vi.fn();
        const { unmount } = render(
            <RemoteSnowLumaQrLoginDialog
                botId="123456"
                open
                isOnline
                onOpenChange={onOpenChange}
                api={api}
            />,
        );
        await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
        expect(api.qrLoginSnapshot).not.toHaveBeenCalled();
        expect(screen.queryByRole('img', { name: 'QQ 登录二维码' })).toBeNull();
        unmount();
    });

    it('closes the completed login instead of offering another refresh', async () => {
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => ({ ...material, state: 'logged_in' as const })),
            closeQrLogin: vi.fn(async () => {}),
        };
        const onOpenChange = vi.fn();
        const { unmount } = render(
            <RemoteSnowLumaQrLoginDialog
                botId="123456"
                open
                isOnline={false}
                onOpenChange={onOpenChange}
                onOpenVnc={vi.fn()}
                api={api}
            />,
        );
        await waitFor(() => expect(screen.getByRole('button', { name: '完成' })).toBeTruthy());
        expect(screen.queryByRole('button', { name: '打开 QQ 界面' })).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: '完成' }));
        expect(onOpenChange).toHaveBeenCalledWith(false);
        unmount();
    });

    it('opens the fallback after closing an unavailable scan session', async () => {
        const api: RemoteQrLoginApi = {
            qrLoginSnapshot: vi.fn(async () => ({ ...material, state: 'unavailable' as const })),
            closeQrLogin: vi.fn(async () => {}),
        };
        const onOpenChange = vi.fn();
        const onOpenVnc = vi.fn();
        const { unmount } = render(
            <RemoteSnowLumaQrLoginDialog
                botId="123456"
                open
                isOnline={false}
                onOpenChange={onOpenChange}
                onOpenVnc={onOpenVnc}
                api={api}
            />,
        );
        await waitFor(() =>
            expect(screen.getByRole('status').textContent).toBe('请打开 QQ 界面继续登录'),
        );
        expect(screen.queryByRole('button', { name: '刷新二维码' })).toBeNull();
        fireEvent.click(screen.getByRole('button', { name: '打开 QQ 界面' }));
        expect(onOpenChange).toHaveBeenCalledWith(false);
        expect(onOpenVnc).toHaveBeenCalledTimes(1);
        unmount();
    });
});
