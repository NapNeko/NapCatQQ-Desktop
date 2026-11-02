import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import { createDefaultBotConfig } from '../../../../core/domain/bot/config-defaults';
import { useBotCardController } from './useBotCardController';
import type { BotCardProps } from './BotCard.types';

vi.mock('../../../../hooks/preferences/featureTogglesStore', () => ({
    useFeatureEnabled: () => true,
}));
vi.mock('../../../../hooks/debug/useDebugConsoleEnabled', () => ({
    useDebugConsoleEnabled: () => true,
}));
vi.mock('../../../../hooks/webui/useRemoteSnowlumaQrLogin', () => ({
    useRemoteSnowlumaQrEnabled: () => true,
}));
vi.mock('../../../../hooks/remote/useIsHostReachable', async (importOriginal) => ({
    ...(await importOriginal<typeof import('../../../../hooks/remote/useIsHostReachable')>()),
    useServerProfiles: () => ({
        data: [{ id: 'linux', name: 'Linux', host: 'example.com', state: 'connected' }],
    }),
}));

afterEach(cleanup);

function cardProps(): BotCardProps {
    const config = createDefaultBotConfig();
    config.bot.backend_type = 'snowluma';
    config.bot.runtime_target = 'linux';
    return {
        bot: {
            bot_id: '12345',
            state: 'running',
            revision: 1,
            token_generation: 1,
            pending_restart: false,
        },
        config,
        flavor: 'snowluma',
        isBatchMode: false,
        isSelected: false,
        onStart: vi.fn(),
        onStop: vi.fn(),
        onConfigure: vi.fn(),
        onViewLogs: vi.fn(),
        onViewMetrics: vi.fn(),
        onToggleSelect: vi.fn(),
        onOpenWebui: vi.fn(),
    };
}

describe('卡片操作与弹窗生命周期', () => {
    it('同一帧从两个入口请求重建时只发一次，完成后恢复可用', async () => {
        let finish!: () => void;
        const props = cardProps();
        props.onRetrySnowlumaUi = vi.fn(
            () =>
                new Promise<void>((resolve) => {
                    finish = resolve;
                }),
        );
        const { result } = renderHook(() => useBotCardController(props));
        act(() => {
            result.current.onAction('retryUi');
            result.current.onAction('retryUi');
        });
        expect(props.onRetrySnowlumaUi).toHaveBeenCalledTimes(1);
        expect(result.current.actions.find((action) => action.id === 'retryUi')?.disabled).toBe(
            true,
        );
        await act(async () => finish());
        expect(result.current.actions.find((action) => action.id === 'retryUi')?.disabled).toBe(
            false,
        );
    });

    it('登录成功或停止后关闭远端扫码，并拒绝调用已不可用的入口', () => {
        const props = cardProps();
        const { result, rerender } = renderHook((value) => useBotCardController(value), {
            initialProps: props,
        });
        act(() => result.current.onAction('remoteQr'));
        expect(result.current.dialogs.remoteQrOpen).toBe(true);
        rerender({ ...props, snowlumaLoginState: 'logged_in' });
        expect(result.current.dialogs.remoteQrOpen).toBe(false);
        act(() => result.current.onAction('remoteQr'));
        expect(result.current.dialogs.remoteQrOpen).toBe(false);
        rerender({ ...props, bot: { ...props.bot, state: 'stopped' } });
        act(() => result.current.onAction('stop'));
        expect(props.onStop).not.toHaveBeenCalled();
    });
});
