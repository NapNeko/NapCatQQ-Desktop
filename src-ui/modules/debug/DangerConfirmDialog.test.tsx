// DangerConfirmDialog 的挂载节奏（F9-M4）：open 变 false 不能立刻卸内容，
// 退场动画放的是完整内容而不是空框；动画播完（onExited）才真卸。

import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render, screen, waitFor } from '@testing-library/react';
import { preferencesStore } from '../../core/domain/settings/preferencesStore';
import { DangerConfirmDialog, _resetDangerSkipsForTests } from './DangerConfirmDialog';

const BODY_MARK = '本次不再询问（到程序退出）';

function TestDialog({ open }: { open: boolean }) {
    return (
        <DangerConfirmDialog
            open={open}
            onOpenChange={() => {}}
            botId="10001"
            botName="甲"
            action="delete_msg"
            params={{}}
            onConfirm={() => {}}
        />
    );
}

afterEach(() => {
    cleanup();
    preferencesStore.reset();
    _resetDangerSkipsForTests();
});

describe('DangerConfirmDialog', () => {
    it('关的动画期间内容还在，退场播完才卸', async () => {
        preferencesStore.setMotionEnabled(true);
        const { rerender } = render(<TestDialog open />);
        expect(await screen.findByText(BODY_MARK)).toBeInTheDocument();
        rerender(<TestDialog open={false} />);
        // 刚关的那一刻内容还在：退场播的是完整内容。老写法 open && 在这一步就卸空了。
        expect(screen.getByText(BODY_MARK)).toBeInTheDocument();
        await waitFor(() => expect(screen.queryByText(BODY_MARK)).not.toBeInTheDocument(), {
            timeout: 3000,
        });
    });
});
