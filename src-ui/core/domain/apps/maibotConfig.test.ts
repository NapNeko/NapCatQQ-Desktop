import { describe, expect, it } from 'vitest';
import {
    MAIBOT_PLACEHOLDER_API_KEY,
    maibotChatDropsEverything,
    maibotChatScope,
    maibotDefaultChat,
    maibotDefaultConfig,
    maibotModelSetupIssue,
    renameMaiBotModel,
    renameMaiBotProvider,
    validateMaiBotConfig,
} from './maibotConfig';

describe('maibotChatDropsEverything', () => {
    it('flags the upstream default of empty whitelists', () => {
        expect(maibotChatDropsEverything(maibotDefaultChat())).toBe(true);
    });

    it('clears once a group is allowed, the filter is off, or a side is a blacklist', () => {
        expect(maibotChatDropsEverything({ ...maibotDefaultChat(), group_list: ['123'] })).toBe(false);
        expect(maibotChatDropsEverything({ ...maibotDefaultChat(), enable_chat_list_filter: false })).toBe(false);
        expect(maibotChatDropsEverything({ ...maibotDefaultChat(), private_list_type: 'blacklist' })).toBe(false);
    });
});

describe('maibotChatScope', () => {
    it('describes both sides in plain words', () => {
        expect(maibotChatScope(maibotDefaultChat())).toBe('不回群，不回私聊');
        expect(maibotChatScope({ ...maibotDefaultChat(), group_list: ['1', '2'], private_list_type: 'blacklist' })).toBe(
            '2 个群，所有私聊',
        );
        expect(maibotChatScope({ ...maibotDefaultChat(), enable_chat_list_filter: false })).toBe('所有群聊和私聊');
    });
});

describe('validateMaiBotConfig', () => {
    it('accepts the default config', () => {
        expect(validateMaiBotConfig(maibotDefaultConfig(23001))).toEqual([]);
    });

    it('rejects equal ports and non-numeric ids with backend paths', () => {
        const cfg = maibotDefaultConfig(23001);
        cfg.bot.maim_message.ws_server_port = 23001;
        cfg.adapter!.chat.group_list = ['12a'];
        expect(validateMaiBotConfig(cfg).map((i) => i.path)).toEqual([
            'bot/maim_message/ws_server_port',
            'adapter/chat/group_list',
        ]);
    });

    it('checks schema ranges and literal options with the backend wording', () => {
        const cfg = maibotDefaultConfig(23001);
        cfg.bot.personality.multiple_probability = 1.5;
        cfg.bot.chat.reply_timing.talk_value_rules[0]!.rule_type = 'channel';
        // 普通字符串挂的 options 只是建议值，不拦
        cfg.models.model_task_config.replyer.selection_strategy = 'custom';
        expect(validateMaiBotConfig(cfg)).toEqual([
            { path: 'bot/personality/multiple_probability', message: '要在 0 到 1 之间' },
            {
                path: 'bot/chat/reply_timing/talk_value_rules/0/rule_type',
                message: '只能是 group / private 之一，现在是 "channel"',
            },
        ]);
    });

    it('catches broken references between providers, models and tasks', () => {
        const cfg = maibotDefaultConfig(23001);
        cfg.models.models[0]!.api_provider = 'Gone';
        cfg.models.models[2]!.name = cfg.models.models[1]!.name;
        cfg.models.model_task_config.planner.model_list = ['nope'];
        expect(validateMaiBotConfig(cfg).map((i) => i.path)).toEqual([
            'models/models/0/api_provider',
            'models/models/2/name',
            'models/model_task_config/planner/model_list/0',
            // 被改名的 flash 原来给杂务用，这下也找不到了
            'models/model_task_config/utils/model_list/0',
        ]);
    });
});

describe('maibotModelSetupIssue', () => {
    it('walks from the placeholder key to ready', () => {
        const models = maibotDefaultConfig(23001).models;
        expect(maibotModelSetupIssue(models)).toBe('placeholder_key');
        models.api_providers[0]!.api_key = 'sk-real';
        expect(maibotModelSetupIssue(models)).toBeNull();
        models.model_task_config.utils.model_list = [];
        expect(maibotModelSetupIssue(models)).toBe('no_task_model');
        expect(maibotModelSetupIssue({ ...models, api_providers: [] })).toBe('no_provider');
    });

    it('ignores a placeholder on a provider no required task uses', () => {
        const models = maibotDefaultConfig(23001).models;
        models.api_providers[0]!.api_key = 'sk-real';
        models.api_providers.push({ ...models.api_providers[0]!, name: 'Spare', api_key: MAIBOT_PLACEHOLDER_API_KEY });
        expect(maibotModelSetupIssue(models)).toBeNull();
    });
});

describe('rename references', () => {
    it('moves models to a renamed provider and tasks to a renamed model', () => {
        const models = maibotDefaultConfig(23001).models;
        models.api_providers[0]!.name = 'DS';
        const moved = renameMaiBotProvider(models, 'DeepSeek', 'DS');
        expect(moved.models.map((m) => m.api_provider)).toEqual(['DS', 'DS', 'DS']);

        moved.models[2]!.name = 'flash';
        const tasks = renameMaiBotModel(moved, 'deepseek-v4-flash', 'flash').model_task_config;
        expect(tasks.planner.model_list).toEqual(['flash']);
        expect(tasks.utils.model_list).toEqual(['flash']);
    });

    it('leaves references alone while another entry still has the old name', () => {
        const models = maibotDefaultConfig(23001).models;
        models.api_providers.push({ ...models.api_providers[0]!, name: 'Other' });
        models.api_providers[1]!.name = 'DeepSeek';
        models.api_providers[0]!.name = 'Renamed';
        expect(renameMaiBotProvider(models, 'DeepSeek', 'Renamed')).toBe(models);
    });
});

describe('maibotDefaultConfig', () => {
    it('carries the full upstream defaults with the instance ports', () => {
        const cfg = maibotDefaultConfig(23001);
        expect(cfg.bot.webui.port).toBe(23001);
        expect(cfg.bot.maim_message.ws_server_port).toBe(23002);
        expect(cfg.bot.chat.reply_timing.talk_value_rules).toHaveLength(2);
        expect(cfg.models.api_providers[0]?.api_key).toBe('your-api-key');
        // 每次都是新副本，改了不会串到下一次
        cfg.bot.personality.personality = 'x';
        expect(maibotDefaultConfig(23001).bot.personality.personality).not.toBe('x');
    });
});
