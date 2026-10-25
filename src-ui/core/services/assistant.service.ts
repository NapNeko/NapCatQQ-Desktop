// 诊断助手命令与独立的流式 Channel。

import { Channel, invoke, isTauri } from '../ipc/transport';
import type {
    AssistantEvent,
    AssistantLoginPlan,
    AssistantModelStatus,
    AssistantProvider,
    AssistantProviderStatus,
    AssistantQuestionAnswer,
    AssistantSession,
    AssistantSessionList,
    AssistantSettings,
} from '../domain/assistant/types';
import { assistantMock } from '../ipc/mock/assistant.mock';

export const assistantService = {
    preview: !isTauri,
    settings: (): Promise<AssistantSettings> =>
        isTauri ? invoke('assistant_settings') : assistantMock.settings(),
    saveSettings: (settings: AssistantSettings): Promise<void> =>
        isTauri
            ? invoke('assistant_save_settings', { settings })
            : assistantMock.saveSettings(settings),
    modelStatus: (): Promise<AssistantModelStatus> =>
        isTauri ? invoke('assistant_model_status') : assistantMock.modelStatus(),
    setApiKey: (apiKey: string): Promise<AssistantModelStatus> =>
        isTauri ? invoke('assistant_set_api_key', { apiKey }) : assistantMock.setApiKey(apiKey),
    clearApiKey: (): Promise<AssistantModelStatus> =>
        isTauri ? invoke('assistant_clear_api_key') : assistantMock.clearApiKey(),
    providers: (): Promise<AssistantProviderStatus[]> =>
        isTauri ? invoke('assistant_providers') : assistantMock.providers(),
    listSessions: (): Promise<AssistantSessionList> =>
        isTauri ? invoke('assistant_list_sessions') : assistantMock.listSessions(),
    getSession: (sessionId: string): Promise<AssistantSession> =>
        isTauri
            ? invoke('assistant_get_session', { sessionId })
            : assistantMock.getSession(sessionId),
    createSession: (provider: AssistantProvider, botId: string): Promise<AssistantSession> =>
        isTauri
            ? invoke('assistant_create_session', { provider, botId })
            : assistantMock.createSession(provider, botId),
    sendMessage: (sessionId: string, text: string): Promise<void> =>
        isTauri
            ? invoke('assistant_send_message', { sessionId, text })
            : assistantMock.sendMessage(sessionId, text),
    cancel: (sessionId: string): Promise<void> =>
        isTauri
            ? invoke('assistant_cancel_session', { sessionId })
            : assistantMock.cancel(sessionId),
    close: (sessionId: string): Promise<void> =>
        isTauri ? invoke('assistant_close_session', { sessionId }) : assistantMock.close(sessionId),
    delete: (sessionId: string): Promise<void> =>
        isTauri
            ? invoke('assistant_delete_session', { sessionId })
            : assistantMock.delete(sessionId),
    loginPlan: (sessionId: string): Promise<AssistantLoginPlan> =>
        isTauri
            ? invoke('assistant_login_plan', { sessionId })
            : Promise.reject(new Error('浏览器预览不执行登录，请在桌面端操作')),
    respondPermission: (
        sessionId: string,
        requestId: string,
        optionId: string | null,
    ): Promise<void> =>
        isTauri
            ? invoke('assistant_respond_permission', { sessionId, requestId, optionId })
            : assistantMock.respondPermission(sessionId, requestId, optionId),
    respondQuestion: (
        sessionId: string,
        requestId: string,
        answer: AssistantQuestionAnswer,
    ): Promise<void> =>
        isTauri
            ? invoke('assistant_respond_question', { sessionId, requestId, answer })
            : assistantMock.respondQuestion(sessionId, requestId, answer),
    async subscribe(listener: (event: AssistantEvent) => void): Promise<() => void> {
        if (!isTauri) return assistantMock.subscribe(listener);
        const channel = new Channel<AssistantEvent>();
        channel.onmessage = listener;
        await invoke('assistant_subscribe', { onEvent: channel });
        return () => {
            channel.onmessage = () => {};
        };
    },
};
