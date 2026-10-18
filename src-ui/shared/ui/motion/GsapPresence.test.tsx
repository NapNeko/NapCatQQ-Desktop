import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import { afterEach, describe, expect, it, vi } from 'vitest';
import gsap from 'gsap';
import { preferencesStore } from '../../../core/domain/settings/preferencesStore';
import { GsapPresence, type EnterFn } from './GsapPresence';

afterEach(() => {
    cleanup();
    preferencesStore.reset();
});

const enter: EnterFn = (el) => gsap.to(el, { opacity: 1, duration: 10, paused: true });

describe('Presence 生命周期', () => {
    it('快速切回时取消旧退出，迟到回调不会卸载内容或通知已退出', () => {
        const done = vi.fn();
        let exit: gsap.core.Tween;
        const leave: EnterFn = (el) => {
            exit = gsap.to(el, { opacity: 0, duration: 10, paused: true });
            return exit;
        };
        const view = (visible: boolean) => (
            <GsapPresence visible={visible} onEnter={enter} onExit={leave} onExited={done}>
                <div>保留内容</div>
            </GsapPresence>
        );
        const { rerender, unmount } = render(view(true));
        rerender(view(false));
        const lateComplete = exit!.eventCallback('onComplete');
        rerender(view(true));
        act(() => lateComplete?.());
        expect(screen.getByText('保留内容')).toBeVisible();
        expect(done).not.toHaveBeenCalled();
        rerender(view(false));
        unmount();
        expect(done).not.toHaveBeenCalled();
    });

    it('keepMounted 首次展开才挂载，收起后保留本地草稿', () => {
        let mounts = 0;
        function Draft() {
            const [value, setValue] = useState(() => {
                mounts += 1;
                return '';
            });
            return (
                <input aria-label="草稿" value={value} onChange={(e) => setValue(e.target.value)} />
            );
        }
        let exit: gsap.core.Tween;
        const leave: EnterFn = (el) => {
            exit = gsap.to(el, { opacity: 0, duration: 10, paused: true });
            return exit;
        };
        const view = (visible: boolean) => (
            <GsapPresence visible={visible} keepMounted onEnter={enter} onExit={leave}>
                <div>
                    <Draft />
                </div>
            </GsapPresence>
        );
        const { rerender } = render(view(false));
        expect(mounts).toBe(0);
        rerender(view(true));
        fireEvent.change(screen.getByRole('textbox', { name: '草稿' }), {
            target: { value: '未保存' },
        });
        rerender(view(false));
        act(() => {
            exit!.progress(1);
        });
        expect(screen.queryByRole('textbox', { name: '草稿' })).toBeNull();
        expect(screen.getByLabelText('草稿')).toHaveValue('未保存');
        rerender(view(true));
        expect(screen.getByRole('textbox', { name: '草稿' })).toHaveValue('未保存');
        expect(mounts).toBe(1);
    });

    it('关闭动效时立即退出，不留下透明占位', () => {
        preferencesStore.setMotionEnabled(false);
        const done = vi.fn();
        const view = (visible: boolean) => (
            <GsapPresence visible={visible} onExited={done}>
                <div>内容</div>
            </GsapPresence>
        );
        const { rerender } = render(view(true));
        rerender(view(false));
        expect(screen.queryByText('内容')).toBeNull();
        expect(done).toHaveBeenCalledOnce();
    });
});
