// @vitest-environment jsdom

import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { BotMigrationDialog } from './BotMigrationDialog';

const migration = vi.hoisted(() => ({
    available: true,
    running: true,
    job: {
        id: 'migration-1',
        bot_id: '10001',
        source_host_id: 'remote:source',
        target_host_id: 'remote:target',
        phase: 'awaiting_login',
        message: '正在核实目标状态',
        apps: [],
        error: null,
        rollback_available: true,
        target_started: true,
        updated_at_ms: 1n,
    },
    jobQuery: { isPending: false, isSuccess: true, isFetching: false, error: null },
    previewQuery: { data: undefined, isFetching: false, isError: false, error: null },
    startMutation: { isPending: false, error: null, reset: vi.fn(), mutate: vi.fn() },
    rollbackMutation: { isPending: false, error: null, reset: vi.fn(), mutate: vi.fn() },
}));
vi.mock('../../../hooks/bot/useBotMigration', () => ({ useBotMigration: () => migration }));
vi.mock('../../../hooks/remote/useIsHostReachable', () => ({
    useServerProfiles: () => ({ data: [], isSuccess: true, isPending: false, error: null }),
}));

beforeEach(() => {
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
    migration.rollbackMutation.mutate.mockClear();
});
afterEach(() => {
    cleanup();
    act(() => preferencesStore.reset());
});

describe('Bot 迁移报告', () => {
    it('回退按需展开，重新打开窗口需要再次确认目标端数据不会回拷', () => {
        const props = {
            botId: '10001',
            sourceRuntimeTarget: 'remote:source',
            open: true,
            onOpenChange: vi.fn(),
        };
        const { rerender } = render(<BotMigrationDialog {...props} />);
        expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
        fireEvent.click(screen.getByRole('button', { name: '回退到源服务器' }));
        expect(screen.getByRole('button', { name: '确认回退' })).toBeDisabled();
        fireEvent.click(screen.getByRole('checkbox'));
        expect(screen.getByRole('button', { name: '确认回退' })).toBeEnabled();
        rerender(<BotMigrationDialog {...props} open={false} />);
        rerender(<BotMigrationDialog {...props} />);
        fireEvent.click(screen.getByRole('button', { name: '回退到源服务器' }));
        expect(screen.getByRole('checkbox')).not.toBeChecked();
        expect(screen.getByRole('button', { name: '确认回退' })).toBeDisabled();
        expect(migration.rollbackMutation.mutate).not.toHaveBeenCalled();
    });
});
