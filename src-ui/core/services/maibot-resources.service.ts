// 麦麦 WebUI 里两份主配置之外的东西（提示词、表情包、学到的表达 / 黑话、人物、长期记忆…）的 IPC。
// 命令字面量只在此文件出现（R3）；浏览器预览走 mock。

import { invoke, isTauri } from '../ipc/transport';
import type {
    MaiBotExpressionAction,
    MaiBotExpressionOverview,
    MaiBotExpressionPage,
    MaiBotExpressionQuery,
    MaiBotJargonAction,
    MaiBotJargonOverview,
    MaiBotJargonPage,
    MaiBotJargonQuery,
    MaiBotPromptAction,
    MaiBotPromptCatalog,
    MaiBotPromptFile,
    MaiBotResourceDone,
} from '../ipc/types';
import { peekMockAppInstance } from '../ipc/mock/app-framework.mock';
import { mockMaiBotLearning } from '../ipc/mock/maibot-learning.mock';
import { mockMaiBotPrompts } from '../ipc/mock/maibot-prompts.mock';

const mockInst = peekMockAppInstance;

export const maibotResourcesService = {
    promptCatalog: async (instanceId: string): Promise<MaiBotPromptCatalog> => {
        if (!isTauri) return mockMaiBotPrompts.catalog(mockInst(instanceId));
        return invoke<MaiBotPromptCatalog>('maibot_prompt_catalog', { instanceId });
    },

    promptFile: async (instanceId: string, language: string, name: string): Promise<MaiBotPromptFile> => {
        if (!isTauri) return mockMaiBotPrompts.file(mockInst(instanceId), language, name);
        return invoke<MaiBotPromptFile>('maibot_prompt_file', { instanceId, language, name });
    },

    promptVersion: async (instanceId: string, language: string, name: string, versionId: string): Promise<string> => {
        if (!isTauri) return mockMaiBotPrompts.version(mockInst(instanceId), language, name, versionId);
        return invoke<string>('maibot_prompt_version', { instanceId, language, name, versionId });
    },

    promptAction: async (instanceId: string, action: MaiBotPromptAction): Promise<MaiBotPromptFile> => {
        if (!isTauri) return mockMaiBotPrompts.action(mockInst(instanceId), action);
        return invoke<MaiBotPromptFile>('maibot_prompt_action', { instanceId, action });
    },

    expressions: async (instanceId: string, query: MaiBotExpressionQuery): Promise<MaiBotExpressionPage> => {
        if (!isTauri) return mockMaiBotLearning.expressions(mockInst(instanceId), query);
        return invoke<MaiBotExpressionPage>('maibot_expressions', { instanceId, query });
    },

    expressionOverview: async (instanceId: string): Promise<MaiBotExpressionOverview> => {
        if (!isTauri) return mockMaiBotLearning.expressionOverview(mockInst(instanceId));
        return invoke<MaiBotExpressionOverview>('maibot_expression_overview', { instanceId });
    },

    expressionAction: async (instanceId: string, action: MaiBotExpressionAction): Promise<MaiBotResourceDone> => {
        if (!isTauri) return mockMaiBotLearning.expressionAction(mockInst(instanceId), action);
        return invoke<MaiBotResourceDone>('maibot_expression_action', { instanceId, action });
    },

    jargons: async (instanceId: string, query: MaiBotJargonQuery): Promise<MaiBotJargonPage> => {
        if (!isTauri) return mockMaiBotLearning.jargons(mockInst(instanceId), query);
        return invoke<MaiBotJargonPage>('maibot_jargons', { instanceId, query });
    },

    jargonOverview: async (instanceId: string): Promise<MaiBotJargonOverview> => {
        if (!isTauri) return mockMaiBotLearning.jargonOverview(mockInst(instanceId));
        return invoke<MaiBotJargonOverview>('maibot_jargon_overview', { instanceId });
    },

    jargonAction: async (instanceId: string, action: MaiBotJargonAction): Promise<MaiBotResourceDone> => {
        if (!isTauri) return mockMaiBotLearning.jargonAction(mockInst(instanceId), action);
        return invoke<MaiBotResourceDone>('maibot_jargon_action', { instanceId, action });
    },
};
