// 麦麦 WebUI 里两份主配置之外的东西（提示词、表情包、学到的表达 / 黑话、人物、长期记忆…）的 IPC。
// 命令字面量只在此文件出现（R3）；浏览器预览走 mock。

import { invoke, isTauri } from '../ipc/transport';
import type { MaiBotPromptAction, MaiBotPromptCatalog, MaiBotPromptFile } from '../ipc/types';
import { peekMockAppInstance } from '../ipc/mock/app-framework.mock';
import { mockMaiBotPrompts } from '../ipc/mock/maibot-prompts.mock';

export const maibotResourcesService = {
    promptCatalog: async (instanceId: string): Promise<MaiBotPromptCatalog> => {
        if (!isTauri) return mockMaiBotPrompts.catalog(peekMockAppInstance(instanceId));
        return invoke<MaiBotPromptCatalog>('maibot_prompt_catalog', { instanceId });
    },

    promptFile: async (instanceId: string, language: string, name: string): Promise<MaiBotPromptFile> => {
        if (!isTauri) return mockMaiBotPrompts.file(peekMockAppInstance(instanceId), language, name);
        return invoke<MaiBotPromptFile>('maibot_prompt_file', { instanceId, language, name });
    },

    promptVersion: async (instanceId: string, language: string, name: string, versionId: string): Promise<string> => {
        if (!isTauri) return mockMaiBotPrompts.version(peekMockAppInstance(instanceId), language, name, versionId);
        return invoke<string>('maibot_prompt_version', { instanceId, language, name, versionId });
    },

    promptAction: async (instanceId: string, action: MaiBotPromptAction): Promise<MaiBotPromptFile> => {
        if (!isTauri) return mockMaiBotPrompts.action(peekMockAppInstance(instanceId), action);
        return invoke<MaiBotPromptFile>('maibot_prompt_action', { instanceId, action });
    },
};
