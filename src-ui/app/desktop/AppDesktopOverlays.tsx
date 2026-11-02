// 协议确认、首次入门和引导续步属于同一生命周期；不将状态提升到导航壳。
import { useCallback, useEffect, useRef, useState } from 'react';
import type { AppRoute } from '../../core/domain/ui/route';
import { useDesktopConsentGate } from '../../hooks/desktop/useDesktopConsentGate';
import { useOnboardingGate } from '../../hooks/desktop/useOnboardingGate';
import { registerOnboardingHost } from '../../hooks/desktop/onboardingHost';
import { registerDesktopConsentHost } from '../../hooks/desktop/desktopConsentHost';
import { useFrameworkTour } from '../../hooks/desktop/useFrameworkTour';
import { useGlobalInfoBars } from '../../hooks/ui/useGlobalInfoBars';
import { InfoBarStack } from '../../shared/ui/InfoBarStack';
import { DesktopConsentDialog } from '../../shared/components/next/DesktopConsentDialog';
import { OnboardingDialog } from '../../shared/components/next/OnboardingDialog';
import { ONBOARDING_GUIDE_STEP_IDS } from '../../shared/components/next/onboardingSteps';
import { OnboardingContinueDialog } from '../../shared/components/next/OnboardingContinueDialog';
import { SpotlightTour } from '../../shared/components/next/SpotlightTour';

export function AppDesktopOverlays({
    navigate,
    setSidebarCollapsed,
}: {
    navigate: (route: AppRoute) => void;
    setSidebarCollapsed: (collapsed: boolean) => void;
}) {
    const desktopConsent = useDesktopConsentGate();
    const { ensureConsent } = desktopConsent;
    const onboarding = useOnboardingGate();
    const consentWasBlockingRef = useRef(false);

    useEffect(() => {
        void (async () => {
            const ok = await desktopConsent.promptAtStartup();
            // 已同意：直接尝试入门选择；未同意则等 accept 后由 blocking 回落触发
            if (ok) await onboarding.promptAfterConsent();
        })();
        // 仅挂载时检查一次
        // eslint-disable-next-line react-hooks/exhaustive-deps -- startup once
    }, []);

    useEffect(() => {
        if (desktopConsent.blocking) {
            consentWasBlockingRef.current = true;
            return;
        }
        if (!consentWasBlockingRef.current) return;
        if (desktopConsent.open) return;
        consentWasBlockingRef.current = false;
        void onboarding.promptAfterConsent();
        // eslint-disable-next-line react-hooks/exhaustive-deps -- only react to consent gate
    }, [desktopConsent.blocking, desktopConsent.open]);

    useEffect(() => {
        onboarding.setNavigate(navigate);
        return () => onboarding.setNavigate(null);
        // eslint-disable-next-line react-hooks/exhaustive-deps -- setNavigate stable
    }, [navigate]);

    const frameworkTour = useFrameworkTour({
        navigate,
        setSidebarCollapsed,
    });

    /** 组件遮罩结束后的「接下来」介绍层（风格对齐第一步） */
    const [continueOpen, setContinueOpen] = useState(false);

    /** 整条引导：Dialog 认路结束后接组件页遮罩（NC/SL + 演示远端） */
    const continueOnboardingFlow = useCallback(async () => {
        await onboarding.finishGuide([...ONBOARDING_GUIDE_STEP_IDS]);
        setContinueOpen(false);
        await frameworkTour.start({ mode: 'full' });
    }, [onboarding, frameworkTour]);

    const handleFrameworkTourClose = useCallback(
        (reason: 'skip' | 'done') => {
            const { shouldOfferContinue } = frameworkTour.close(reason);
            if (shouldOfferContinue) {
                setContinueOpen(true);
            }
        },
        [frameworkTour],
    );

    const handleContinueToBots = useCallback(async () => {
        setContinueOpen(false);
        await frameworkTour.start({ mode: 'bots' });
    }, [frameworkTour]);

    const handleContinueFinish = useCallback(() => {
        setContinueOpen(false);
        navigate('components');
    }, [navigate]);

    useEffect(() => {
        // 设置「重新查看入门」= 从 Dialog 再走一遍，走完仍接遮罩
        registerOnboardingHost(() => onboarding.openFromSettings());
        return () => registerOnboardingHost(null);
        // eslint-disable-next-line react-hooks/exhaustive-deps -- host once per mount identity
    }, [onboarding.openFromSettings]);

    useEffect(() => {
        // Bot 列表 / 批量启动等复用 App 级协议门禁，禁止页面内再挂一份 gate
        registerDesktopConsentHost((action) => ensureConsent(action));
        return () => registerDesktopConsentHost(null);
    }, [ensureConsent]);

    return (
        <>
            <DesktopConsentDialog
                open={desktopConsent.open}
                mode={desktopConsent.mode}
                payload={desktopConsent.payload}
                submitting={desktopConsent.submitting}
                onAccept={() => void desktopConsent.accept()}
                onClose={desktopConsent.close}
            />

            <OnboardingDialog
                open={onboarding.open}
                mode={onboarding.mode}
                submitting={onboarding.submitting || frameworkTour.open}
                onExplore={() => void onboarding.chooseExplore()}
                onSkip={() => void onboarding.chooseSkip()}
                onCloseGuide={() => void onboarding.closeGuide()}
                onGoComponents={() => void continueOnboardingFlow()}
                onGoBots={() => void continueOnboardingFlow()}
                onFinish={() => void continueOnboardingFlow()}
                onDismissToApp={() => {
                    void (async () => {
                        await onboarding.finishGuide([...ONBOARDING_GUIDE_STEP_IDS]);
                        navigate('overview');
                    })();
                }}
            />

            <SpotlightTour
                open={frameworkTour.open}
                steps={frameworkTour.spotlightSteps}
                stepIndex={frameworkTour.stepIndex}
                onStepIndexChange={frameworkTour.setStepIndex}
                onClose={handleFrameworkTourClose}
                onBeforeStep={frameworkTour.onBeforeStep}
            />

            <OnboardingContinueDialog
                open={continueOpen}
                submitting={frameworkTour.open}
                onContinueBots={() => void handleContinueToBots()}
                onFinish={handleContinueFinish}
            />
        </>
    );
}

export function AppNotifications() {
    const { bars, dismiss, remove } = useGlobalInfoBars();
    return <InfoBarStack items={bars} onDismiss={dismiss} onAutoDismiss={remove} />;
}
