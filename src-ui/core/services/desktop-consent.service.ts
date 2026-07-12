// Desktop 用户协议 / 隐私说明 IPC。
// 命令名唯一来源：get_desktop_agreements / accept_desktop_agreements。

import { invoke, isTauri } from '../ipc/transport';
import type { DesktopAgreementsPayload } from '../ipc/generated/DesktopAgreementsPayload';

const MOCK_PAYLOAD: DesktopAgreementsPayload = {
    version: 'browser-preview',
    consent_required: false,
    accepted_at: null,
    documents: [
        {
            id: 'eula',
            title: 'NapCatQQ Desktop 最终用户许可协议（EULA）',
            declared_version: '1.0',
            text: '# 浏览器预览\n\n非 Tauri 环境下不强制协议门禁。',
        },
        {
            id: 'privacy',
            title: 'NapCatQQ Desktop 隐私与数据处理说明',
            declared_version: '1.0',
            text: '# 浏览器预览\n\n隐私说明仅在桌面端完整展示。',
        },
    ],
};

export type { DesktopAgreementsPayload };

export const desktopConsentService = {
    getAgreements: async (): Promise<DesktopAgreementsPayload> => {
        if (!isTauri) return { ...MOCK_PAYLOAD };
        return invoke<DesktopAgreementsPayload>('get_desktop_agreements');
    },

    accept: async (version: string): Promise<DesktopAgreementsPayload> => {
        if (!isTauri) {
            return {
                ...MOCK_PAYLOAD,
                consent_required: false,
                accepted_at: new Date().toISOString(),
                version,
            };
        }
        return invoke<DesktopAgreementsPayload>('accept_desktop_agreements', { version });
    },
};
