// 浏览器 dev 模式下的后端设置 mock。

import type { BackendSettings } from '../../services/settings.service';
import {
    DEFAULT_ONEBOT_MESSAGE,
    DEFAULT_WEBHOOK_BODY,
} from '../../domain/settings/offline-notify-defaults';
import { defaultAppUiPreferencesFromPrefs } from '../../domain/settings/ui-preferences-bridge';
import { DEFAULT_TASK_QUEUE_CLEANUP } from '../../domain/task-queue/cleanup';
import { preferencesStore } from '../../../hooks/preferences/preferencesStore';

export const mockBackendSettings: BackendSettings = {
    botLoginCheckIntervalMs: 5000,
    botOfflineWebHookNotice: false,
    botOfflineEmailNotice: false,
    performanceMonitorEnabled: true,
    performanceMonitorIntervalMs: 1200,
    taskQueueCleanup: { ...DEFAULT_TASK_QUEUE_CLEANUP },
    githubPat: '',
    closeAction: 'close',
    afterCloseUiBehavior: 'delayed_lightweight',
    enterLightweightDelaySecs: 300,
    uiModeOnStartup: 'normal',
    launchOnStartup: false,
    minimizeToTrayCountsAsHidden: true,
    notifyOnOffline: true,
    notifyOnBotCrashed: true,
    notifyOnLoginKicked: true,
    uiPreferences: defaultAppUiPreferencesFromPrefs(preferencesStore.get()),
    remoteHostHealthProbeEnabled: true,
    remoteHostHealthProbeIntervalMs: 30_000,
    webHookChannels: [],
    webHookUrl: '',
    webHookSecret: '',
    webHookJson: DEFAULT_WEBHOOK_BODY,
    webHookMethod: 'POST',
    emailSender: '',
    emailReceiver: '',
    emailToken: '',
    emailSmtpServer: '',
    emailSmtpPort: 465,
    emailEncryption: 'SSL',
    onebotNoticeEnabled: false,
    onebotMessengerBotId: '',
    onebotMessengerBotIds: [],
    onebotTargetType: 'private',
    onebotTargetId: 0,
    onebotTargetIds: [],
    onebotMessageTemplate: DEFAULT_ONEBOT_MESSAGE,
    notifyOnRecovered: false,
    offlineDebounceSeconds: 0,
    offlineDeliveryHistoryLimit: 50,
};
