// 应用级订阅与启动副作用。生命周期独立于当前页和各类弹层。
import { useEffect } from 'react';
import { useComponentActionEventBridge } from '../../hooks/components/useComponentActionBridge';
import { useDockerDeployProgressBridge } from '../../hooks/docker/useDockerDeployProgressBridge';
import { useDockerInstallProgressBridge } from '../../hooks/docker/useDockerInstallProgressBridge';
import { useDeploymentTaskBridge } from '../../hooks/task-queue/useDeploymentTaskBridge';
import { useAppInstanceEventsBridge } from '../../hooks/apps/useAppInstanceEventsBridge';
import { useComponentsWarmup } from '../../hooks/components/useComponents';
import { useHostConnectionEvents } from '../../hooks/remote/useHostConnectionEvents';
import { useHostHealthAlerts } from '../../hooks/remote/useHostHealthAlerts';
import { useAppUiPreferencesBootstrap } from '../../hooks/preferences/useAppUiPreferencesBootstrap';
import { useFeatureEnabled } from '../../hooks/preferences/featureTogglesStore';
import { useAppScreenshotShortcut } from '../../hooks/chat/useChatScreenshot';
import { terminalStore } from '../../hooks/terminal/terminalStore';
import { useBootstrap } from '../../hooks/bootstrap/useBootstrap';
import { useDataLayoutConsolidateAlert } from '../../hooks/bootstrap/useDataLayoutConsolidateAlert';
import { useDesktopUpdateStartupNotice } from '../../hooks/desktop/useDesktopUpdate';

export function AppBackgroundEffects() {
    useComponentActionEventBridge();
    useDockerDeployProgressBridge();
    useDockerInstallProgressBridge();
    useDeploymentTaskBridge();
    useAppInstanceEventsBridge();
    useComponentsWarmup();
    useHostConnectionEvents();
    useHostHealthAlerts();
    useAppUiPreferencesBootstrap();
    useDesktopUpdateStartupNotice();
    const { bootstrap } = useBootstrap();
    useDataLayoutConsolidateAlert(bootstrap);
    const chatEnabled = useFeatureEnabled('chat');
    const terminalEnabled = useFeatureEnabled('terminal');
    useAppScreenshotShortcut(chatEnabled);
    useEffect(() => {
        if (!terminalEnabled) void terminalStore.closeAll();
    }, [terminalEnabled]);
    return null;
}
