import { describe, expect, it } from 'vitest';
import { buildBotCardActions, toolbarActions, type BotCardCapabilities } from './card-actions';

const capabilities: BotCardCapabilities = {
    webui: false,
    webuiTooltip: '',
    novnc: false,
    qr: false,
    remoteQr: false,
    retryUi: true,
    terminal: true,
    debug: true,
};
const input = {
    state: 'stopped' as const,
    actionPending: false,
    transportFailed: false,
    isDocker: true,
    retryingUi: false,
    capabilities,
};

describe('卡片操作入口共享可用性', () => {
    it('断网与启动准备阶段均阻断启动，运行中的停止仍可操作', () => {
        for (const patch of [{ transportFailed: true }, { actionPending: true }]) {
            const actions = buildBotCardActions({ ...input, ...patch });
            expect(actions.find((action) => action.id === 'start')?.disabled).toBe(true);
            expect(
                toolbarActions(actions, 'primary').find((action) => action.id === 'start')
                    ?.disabled,
            ).toBe(true);
        }
        const running = buildBotCardActions({
            ...input,
            state: 'running',
            actionPending: true,
            transportFailed: true,
        });
        expect(running.find((action) => action.id === 'stop')?.disabled).toBe(false);
        expect(running.some((action) => action.id === 'start')).toBe(false);
    });

    it('重建中的禁用状态同时应用到右键和更多工具，停止状态不显示运行期入口', () => {
        const actions = buildBotCardActions({ ...input, retryingUi: true });
        expect(actions.find((action) => action.id === 'retryUi')?.disabled).toBe(true);
        expect(
            toolbarActions(actions, 'more').find((action) => action.id === 'retryUi')?.disabled,
        ).toBe(true);
        expect(actions.some((action) => action.id === 'debug')).toBe(true);
        expect(toolbarActions(actions, 'more').some((action) => action.id === 'debug')).toBe(false);
        expect(
            actions.some((action) => ['webui', 'novnc', 'qr', 'remoteQr'].includes(action.id)),
        ).toBe(false);
    });
});
