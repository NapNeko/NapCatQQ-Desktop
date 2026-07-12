// 新手引导门：consent 通过后询问了解/跳过；设置页可重开内容向导。
// 不挡主界面渲染（与 DesktopConsent 不同）；仅控制 OnboardingDialog。

import { useCallback, useRef, useState } from 'react';
import {
    desktopOnboardingService,
    type DesktopOnboardingPayload,
} from '../../core/services/desktop-onboarding.service';
import { pushInfoBar } from '../ui/globalInfoBarStore';

export type OnboardingDialogMode = 'choice' | 'guide';

type NavigateFn = (route: 'overview' | 'bots' | 'components' | 'docker' | 'remote' | 'tasks' | 'settings') => void;

export function useOnboardingGate() {
    const [open, setOpen] = useState(false);
    const [mode, setMode] = useState<OnboardingDialogMode>('choice');
    const [payload, setPayload] = useState<DesktopOnboardingPayload | null>(null);
    const [submitting, setSubmitting] = useState(false);
    const bootstrappedRef = useRef(false);
    const navigateRef = useRef<NavigateFn | null>(null);

    const setNavigate = useCallback((fn: NavigateFn | null) => {
        navigateRef.current = fn;
    }, []);

    /**
     * 启动时在 consent 放行后调用一次。
     * shouldPromptChoice 时打开选择页；否则不打扰。
     */
    const promptAfterConsent = useCallback(async (): Promise<void> => {
        if (bootstrappedRef.current) return;
        bootstrappedRef.current = true;
        try {
            const next = await desktopOnboardingService.get();
            setPayload(next);
            // 只认配置文件：pending → 选择页；active（上次选了了解未走完）→ 续内容向导
            if (next.shouldPromptChoice || next.state.status === 'pending') {
                setMode('choice');
                setOpen(true);
                return;
            }
            if (next.state.status === 'active') {
                setMode('guide');
                setOpen(true);
            }
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'warning',
                title: '无法加载入门引导状态',
                content: err instanceof Error ? err.message : String(err),
            });
        }
    }, []);

    /** 设置 · 关于：重新打开内容向导（跳过选择页）。 */
    const openFromSettings = useCallback(async (): Promise<void> => {
        setSubmitting(true);
        try {
            const next = await desktopOnboardingService.reopen();
            setPayload(next);
            setMode('guide');
            setOpen(true);
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'danger',
                title: '无法打开入门引导',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            setSubmitting(false);
        }
    }, []);

    const chooseExplore = useCallback(async () => {
        if (submitting) return;
        setSubmitting(true);
        try {
            const next = await desktopOnboardingService.start();
            setPayload(next);
            setMode('guide');
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'danger',
                title: '无法开始入门引导',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            setSubmitting(false);
        }
    }, [submitting]);

    const chooseSkip = useCallback(async () => {
        if (submitting) return;
        setSubmitting(true);
        try {
            const next = await desktopOnboardingService.skip();
            setPayload(next);
            setOpen(false);
        } catch (err: unknown) {
            pushInfoBar({
                tone: 'danger',
                title: '无法保存「跳过」',
                content: err instanceof Error ? err.message : String(err),
            });
        } finally {
            setSubmitting(false);
        }
    }, [submitting]);

    const finishGuide = useCallback(
        async (completedStepIds?: string[]) => {
            if (submitting) return;
            setSubmitting(true);
            try {
                const next = await desktopOnboardingService.complete(completedStepIds);
                setPayload(next);
                setOpen(false);
            } catch (err: unknown) {
                pushInfoBar({
                    tone: 'danger',
                    title: '无法保存入门进度',
                    content: err instanceof Error ? err.message : String(err),
                });
            } finally {
                setSubmitting(false);
            }
        },
        [submitting],
    );

    /** 兼容旧调用；门禁弹窗不再走 Esc/遮罩关闭，正式出口是 finish / skip / 跳转。 */
    const closeGuide = useCallback(async () => {
        if (submitting) return;
        if (mode === 'choice') return;
        await finishGuide(['welcome', 'map', 'path', 'tips', 'go']);
    }, [finishGuide, mode, submitting]);

    const goToRoute = useCallback(
        async (route: Parameters<NavigateFn>[0], completedStepIds?: string[]) => {
            await finishGuide(completedStepIds);
            navigateRef.current?.(route);
        },
        [finishGuide],
    );

    return {
        open,
        mode,
        payload,
        submitting,
        promptAfterConsent,
        openFromSettings,
        chooseExplore,
        chooseSkip,
        finishGuide,
        closeGuide,
        goToRoute,
        setNavigate,
    };
}
