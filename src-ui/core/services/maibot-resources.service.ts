// 麦麦 WebUI 里两份主配置之外的东西（提示词、表情包、学到的表达 / 黑话 / 行为、人物、长期记忆…）的 IPC。
// 命令字面量只在此文件出现，后端改命令名只用改这一处；浏览器预览走 mock。

import { invoke, isTauri, pickImageFiles } from '../ipc/transport';
import type {
    MaiBotBehaviorDetail,
    MaiBotBehaviorOverview,
    MaiBotBehaviorPage,
    MaiBotBehaviorQuery,
    MaiBotEmojiAction,
    MaiBotEmojiImage,
    MaiBotEmojiOverview,
    MaiBotEmojiPage,
    MaiBotEmojiQuery,
    MaiBotEmojiUpload,
    MaiBotEmojiUploadDone,
    MaiBotExpressionAction,
    MaiBotExpressionOverview,
    MaiBotExpressionPage,
    MaiBotExpressionQuery,
    MaiBotJargonAction,
    MaiBotJargonOverview,
    MaiBotJargonPage,
    MaiBotJargonQuery,
    MaiBotLocalImage,
    MaiBotPersonAction,
    MaiBotPersonOverview,
    MaiBotPersonPage,
    MaiBotPersonQuery,
    MaiBotPromptAction,
    MaiBotPromptCatalog,
    MaiBotPromptFile,
    MaiBotResourceDone,
} from '../ipc/types';
import { peekMockAppInstance } from '../ipc/mock/app-framework.mock';
import { mockMaiBotBehaviors } from '../ipc/mock/maibot-behavior.mock';
import { mockMaiBotEmojis } from '../ipc/mock/maibot-emoji.mock';
import { mockMaiBotLearning } from '../ipc/mock/maibot-learning.mock';
import { mockMaiBotPersons } from '../ipc/mock/maibot-person.mock';
import { mockMaiBotPrompts } from '../ipc/mock/maibot-prompts.mock';

const mockInst = peekMockAppInstance;

export const maibotResourcesService = {
    promptCatalog: async (instanceId: string): Promise<MaiBotPromptCatalog> => {
        if (!isTauri) return mockMaiBotPrompts.catalog(mockInst(instanceId));
        return invoke<MaiBotPromptCatalog>('maibot_prompt_catalog', { instanceId });
    },

    promptFile: async (
        instanceId: string,
        language: string,
        name: string,
    ): Promise<MaiBotPromptFile> => {
        if (!isTauri) return mockMaiBotPrompts.file(mockInst(instanceId), language, name);
        return invoke<MaiBotPromptFile>('maibot_prompt_file', { instanceId, language, name });
    },

    promptVersion: async (
        instanceId: string,
        language: string,
        name: string,
        versionId: string,
    ): Promise<string> => {
        if (!isTauri)
            return mockMaiBotPrompts.version(mockInst(instanceId), language, name, versionId);
        return invoke<string>('maibot_prompt_version', { instanceId, language, name, versionId });
    },

    promptAction: async (
        instanceId: string,
        action: MaiBotPromptAction,
    ): Promise<MaiBotPromptFile> => {
        if (!isTauri) return mockMaiBotPrompts.action(mockInst(instanceId), action);
        return invoke<MaiBotPromptFile>('maibot_prompt_action', { instanceId, action });
    },

    expressions: async (
        instanceId: string,
        query: MaiBotExpressionQuery,
    ): Promise<MaiBotExpressionPage> => {
        if (!isTauri) return mockMaiBotLearning.expressions(mockInst(instanceId), query);
        return invoke<MaiBotExpressionPage>('maibot_expressions', { instanceId, query });
    },

    expressionOverview: async (instanceId: string): Promise<MaiBotExpressionOverview> => {
        if (!isTauri) return mockMaiBotLearning.expressionOverview(mockInst(instanceId));
        return invoke<MaiBotExpressionOverview>('maibot_expression_overview', { instanceId });
    },

    expressionAction: async (
        instanceId: string,
        action: MaiBotExpressionAction,
    ): Promise<MaiBotResourceDone> => {
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

    jargonAction: async (
        instanceId: string,
        action: MaiBotJargonAction,
    ): Promise<MaiBotResourceDone> => {
        if (!isTauri) return mockMaiBotLearning.jargonAction(mockInst(instanceId), action);
        return invoke<MaiBotResourceDone>('maibot_jargon_action', { instanceId, action });
    },

    behaviors: async (
        instanceId: string,
        query: MaiBotBehaviorQuery,
    ): Promise<MaiBotBehaviorPage> => {
        if (!isTauri) return mockMaiBotBehaviors.list(mockInst(instanceId), query);
        return invoke<MaiBotBehaviorPage>('maibot_behaviors', { instanceId, query });
    },

    behaviorOverview: async (instanceId: string): Promise<MaiBotBehaviorOverview> => {
        if (!isTauri) return mockMaiBotBehaviors.overview(mockInst(instanceId));
        return invoke<MaiBotBehaviorOverview>('maibot_behavior_overview', { instanceId });
    },

    behavior: async (instanceId: string, id: number): Promise<MaiBotBehaviorDetail> => {
        if (!isTauri) return mockMaiBotBehaviors.detail(mockInst(instanceId), id);
        return invoke<MaiBotBehaviorDetail>('maibot_behavior_detail', { instanceId, id });
    },

    persons: async (instanceId: string, query: MaiBotPersonQuery): Promise<MaiBotPersonPage> => {
        if (!isTauri) return mockMaiBotPersons.list(mockInst(instanceId), query);
        return invoke<MaiBotPersonPage>('maibot_persons', { instanceId, query });
    },

    personOverview: async (instanceId: string): Promise<MaiBotPersonOverview> => {
        if (!isTauri) return mockMaiBotPersons.overview(mockInst(instanceId));
        return invoke<MaiBotPersonOverview>('maibot_person_overview', { instanceId });
    },

    personAction: async (
        instanceId: string,
        action: MaiBotPersonAction,
    ): Promise<MaiBotResourceDone> => {
        if (!isTauri) return mockMaiBotPersons.action(mockInst(instanceId), action);
        return invoke<MaiBotResourceDone>('maibot_person_action', { instanceId, action });
    },

    emojis: async (instanceId: string, query: MaiBotEmojiQuery): Promise<MaiBotEmojiPage> => {
        if (!isTauri) return mockMaiBotEmojis.list(mockInst(instanceId), query);
        return invoke<MaiBotEmojiPage>('maibot_emojis', { instanceId, query });
    },

    emojiOverview: async (instanceId: string): Promise<MaiBotEmojiOverview> => {
        if (!isTauri) return mockMaiBotEmojis.overview(mockInst(instanceId));
        return invoke<MaiBotEmojiOverview>('maibot_emoji_overview', { instanceId });
    },

    emojiAction: async (
        instanceId: string,
        action: MaiBotEmojiAction,
    ): Promise<MaiBotResourceDone> => {
        if (!isTauri) return mockMaiBotEmojis.action(mockInst(instanceId), action);
        return invoke<MaiBotResourceDone>('maibot_emoji_action', { instanceId, action });
    },

    emojiImage: async (
        instanceId: string,
        emojiId: number,
        original: boolean,
    ): Promise<MaiBotEmojiImage> => {
        if (!isTauri) return mockMaiBotEmojis.image(mockInst(instanceId), emojiId, original);
        return invoke<MaiBotEmojiImage>('maibot_emoji_image', { instanceId, emojiId, original });
    },

    emojiUpload: async (
        instanceId: string,
        upload: MaiBotEmojiUpload,
    ): Promise<MaiBotEmojiUploadDone> => {
        if (!isTauri) return mockMaiBotEmojis.upload(mockInst(instanceId), upload);
        return invoke<MaiBotEmojiUploadDone>('maibot_emoji_upload', { instanceId, upload });
    },

    /** 系统对话框挑图；取消给空数组 */
    pickEmojiFiles: async (): Promise<string[]> => {
        if (!isTauri) return mockMaiBotEmojis.pickFiles();
        return pickImageFiles('挑几张表情包');
    },

    /** 上传前看本机的图，不碰实例 */
    localImages: async (paths: string[]): Promise<MaiBotLocalImage[]> => {
        if (!isTauri) return mockMaiBotEmojis.localImages(paths);
        return invoke<MaiBotLocalImage[]>('maibot_local_images', { paths });
    },
};
