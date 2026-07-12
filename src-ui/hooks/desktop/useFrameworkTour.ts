// 框架对比 spotlight：本机 NC/SL + 演示远端依赖。
// full 跑完后由 App 弹「接下来」介绍层，再可选 bots 演示新建流程。
// 结束必须 clearComponentsHostBridge / clearBotTourBridge。

import { useCallback, useMemo, useRef, useState } from 'react';
import {
    BOT_CREATE_STEPS,
    FULL_FRAMEWORK_TOUR_STEPS,
    LOCAL_FRAMEWORK_STEPS,
    REMOTE_DEMO_STEPS,
    type FrameworkTourStep,
} from '../../core/domain/onboarding/frameworkTourSteps';
import { DEMO_REMOTE_HOST_ID } from '../../core/domain/onboarding/tourIds';
import type { FrameworkTourRequest } from './frameworkTourHost';
import {
    clearComponentsHostBridge,
    setComponentsHostBridge,
} from './componentsHostBridge';
import {
    clearBotTourBridge,
    setBotTourBridge,
    type BotTourConfigTab,
} from './botTourBridge';
import type { AppRoute } from '../../shared/components/next/Sidebar';

type NavigateFn = (route: AppRoute) => void;

// 等 DOM 锚点出现（连接 Tab / 路由 lazy 挂载等延迟）。
// attempts 略放宽：业务页 code-split 后首进可能多等几帧 chunk。
function waitForTourTarget(tourId: string, attempts = 80): Promise<boolean> {
    return new Promise((resolve) => {
        let n = 0;
        const tick = () => {
            const el = document.querySelector(`[data-tour-id="${tourId}"]`);
            if (el) {
                const r = el.getBoundingClientRect();
                if (r.width >= 2 && r.height >= 2) {
                    resolve(true);
                    return;
                }
            }
            n += 1;
            if (n >= attempts) {
                resolve(false);
                return;
            }
            requestAnimationFrame(() => requestAnimationFrame(tick));
        };
        tick();
    });
}

/** bots 步骤 id → 配置页 Tab / 是否打开新建 */
function botStepUi(stepId: string): {
    openCreate: boolean;
    forceTab: BotTourConfigTab | null;
    requestList: boolean;
} {
    switch (stepId) {
        case 'bots-nav':
        case 'bots-list':
            return { openCreate: false, forceTab: null, requestList: true };
        case 'bots-create-open':
            // 只在这一步强制打开新建，后续步只切 Tab，避免反复 openCreate 抢布局
            return { openCreate: true, forceTab: 'identity', requestList: false };
        case 'bots-identity':
        case 'bots-runtime':
            return { openCreate: false, forceTab: 'identity', requestList: false };
        case 'bots-connections':
            return { openCreate: false, forceTab: 'connections', requestList: false };
        case 'bots-save':
            return { openCreate: false, forceTab: 'identity', requestList: false };
        case 'bots-after':
            return { openCreate: false, forceTab: null, requestList: true };
        default:
            return { openCreate: false, forceTab: null, requestList: false };
    }
}

export function useFrameworkTour(opts: {
    navigate: NavigateFn;
    setSidebarCollapsed?: (collapsed: boolean) => void;
}) {
    const { navigate, setSidebarCollapsed } = opts;
    const [open, setOpen] = useState(false);
    const [stepIndex, setStepIndex] = useState(0);
    const [steps, setSteps] = useState<FrameworkTourStep[]>([...FULL_FRAMEWORK_TOUR_STEPS]);
    const keepDemoRemoteRef = useRef(false);
    /** 当前这一轮是否 full（本机+远端），用于 close 时是否弹衔接层 */
    const runModeRef = useRef<FrameworkTourRequest['mode']>('full');

    const spotlightSteps = useMemo(
        () =>
            steps.map((s) => ({
                id: s.id,
                target: s.target,
                title: s.title,
                body: s.body,
            })),
        [steps],
    );

    const applyHost = useCallback((hostId: string) => {
        setComponentsHostBridge({
            preferredHostId: hostId,
            includeDemoRemote:
                keepDemoRemoteRef.current || hostId === DEMO_REMOTE_HOST_ID,
            hostSelectionLocked: true,
        });
    }, []);

    const start = useCallback(
        async (req?: FrameworkTourRequest) => {
            const mode = req?.mode ?? 'full';
            runModeRef.current = mode;
            const next =
                mode === 'local'
                    ? [...LOCAL_FRAMEWORK_STEPS]
                    : mode === 'remote'
                        ? [...REMOTE_DEMO_STEPS]
                        : mode === 'bots'
                            ? [...BOT_CREATE_STEPS]
                            : [...FULL_FRAMEWORK_TOUR_STEPS];
            keepDemoRemoteRef.current = mode === 'full' || mode === 'remote';
            setSteps(next);
            setStepIndex(0);
            setSidebarCollapsed?.(false);

            const first = next[0];
            if (first?.phase === 'bots') {
                clearComponentsHostBridge();
                navigate('bots');
                const ui = botStepUi(first.id);
                setBotTourBridge({
                    demoMode: true,
                    openCreate: ui.openCreate,
                    forceTab: ui.forceTab,
                    requestList: ui.requestList,
                });
            } else {
                clearBotTourBridge();
                navigate('components');
                const firstHost = first?.selectHostId ?? 'local';
                setComponentsHostBridge({
                    preferredHostId: firstHost,
                    includeDemoRemote: keepDemoRemoteRef.current,
                    hostSelectionLocked: true,
                });
            }
            await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
            setOpen(true);
        },
        [navigate, setSidebarCollapsed],
    );

    /**
     * 关闭遮罩。
     * - full + done：组件段走完，应弹「接下来」介绍层（返回 shouldOfferContinue）
     * - full + skip / 其它 mode：直接收尾
     */
    const close = useCallback((reason: 'skip' | 'done') => {
        setOpen(false);
        setStepIndex(0);
        keepDemoRemoteRef.current = false;
        clearComponentsHostBridge();
        // 演示新建结束：回列表并清 demo
        setBotTourBridge({
            openCreate: false,
            demoMode: false,
            forceTab: null,
            requestList: true,
        });
        // 下一帧再 clear，让 BotPage 先处理 requestList
        window.setTimeout(() => clearBotTourBridge(), 120);
        const shouldOfferContinue =
            reason === 'done' && (runModeRef.current ?? 'full') === 'full';
        return { shouldOfferContinue };
    }, []);

    const onBeforeStep = useCallback(
        async (_step: { id: string }, index: number) => {
            const full = steps[index];
            if (!full) return;
            setSidebarCollapsed?.(false);

            if (full.phase === 'bots') {
                clearComponentsHostBridge();
                keepDemoRemoteRef.current = false;
                navigate('bots');
                const ui = botStepUi(full.id);
                setBotTourBridge({
                    demoMode: true,
                    openCreate: ui.openCreate,
                    forceTab: ui.forceTab,
                    requestList: ui.requestList,
                });
                // 打开新建 / 切 Tab：TabsContent 非激活不挂载。
                // 连接步：先等 forceTab 生效再等 body 挂上（见 waitForTourTarget）。
                const waitMs =
                    full.id === 'bots-connections'
                        ? 120
                        : ui.openCreate
                            ? 100
                            : 60;
                await new Promise((r) => setTimeout(r, waitMs));
                if (full.id === 'bots-connections') {
                    await waitForTourTarget('bot-connections-body', 48);
                } else if (ui.openCreate) {
                    await waitForTourTarget(full.target, 36);
                }
                await new Promise((r) =>
                    requestAnimationFrame(() => requestAnimationFrame(r)),
                );
                return;
            }

            clearBotTourBridge();
            navigate('components');
            const hostId = full.selectHostId ?? 'local';
            applyHost(hostId);
            await new Promise((r) =>
                setTimeout(r, hostId === DEMO_REMOTE_HOST_ID ? 140 : 60),
            );
            await new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r)));
        },
        [applyHost, navigate, setSidebarCollapsed, steps],
    );

    return {
        open,
        stepIndex,
        setStepIndex,
        spotlightSteps,
        start,
        close,
        onBeforeStep,
    };
}
