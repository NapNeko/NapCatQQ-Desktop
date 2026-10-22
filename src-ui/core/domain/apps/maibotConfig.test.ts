import { describe, expect, it } from 'vitest';
import { MODEL_SCHEMA, newItemFor, nodeAt } from './maibotSchema';
import {
    MAIBOT_PLACEHOLDER_API_KEY,
    MAIBOT_PROVIDER_PRESETS,
    MAIBOT_TASK_KEYS,
    maibotChatDropsEverything,
    maibotChatScope,
    maibotDefaultChat,
    maibotDefaultConfig,
    maibotModelSetupIssue,
    newMaiBotProvider,
    renameMaiBotModel,
    renameMaiBotProvider,
    validateMaiBotConfig,
} from './maibotConfig';

describe('maibotChatDropsEverything', () => {
    it('flags the upstream default of empty whitelists', () => {
        expect(maibotChatDropsEverything(maibotDefaultChat())).toBe(true);
    });

    it('clears once a group is allowed, the filter is off, or a side is a blacklist', () => {
        expect(maibotChatDropsEverything({ ...maibotDefaultChat(), group_list: ['123'] })).toBe(
            false,
        );
        expect(
            maibotChatDropsEverything({ ...maibotDefaultChat(), enable_chat_list_filter: false }),
        ).toBe(false);
        expect(
            maibotChatDropsEverything({ ...maibotDefaultChat(), private_list_type: 'blacklist' }),
        ).toBe(false);
    });
});

describe('maibotChatScope', () => {
    it('describes both sides in plain words', () => {
        expect(maibotChatScope(maibotDefaultChat())).toBe('不回群，不回私聊');
        expect(
            maibotChatScope({
                ...maibotDefaultChat(),
                group_list: ['1', '2'],
                private_list_type: 'blacklist',
            }),
        ).toBe('2 个群，所有私聊');
        expect(maibotChatScope({ ...maibotDefaultChat(), enable_chat_list_filter: false })).toBe(
            '所有群聊和私聊',
        );
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
        models.api_providers.push({
            ...models.api_providers[0]!,
            name: 'Spare',
            api_key: MAIBOT_PLACEHOLDER_API_KEY,
        });
        expect(maibotModelSetupIssue(models)).toBeNull();
    });
});

describe('newMaiBotProvider', () => {
    const preset = (id: string) => MAIBOT_PROVIDER_PRESETS.find((p) => p.id === id)!;

    it('fills the preset over upstream defaults and numbers a taken name', () => {
        const existing = maibotDefaultConfig(23001).models.api_providers;
        const p = newMaiBotProvider(existing, preset('deepseek'));
        expect(p).toMatchObject({
            name: 'DeepSeek 2',
            base_url: 'https://api.deepseek.com',
            client_type: 'openai',
            auth_type: 'bearer',
            api_key: '',
            max_retry: 3,
            default_headers: {},
        });
        // 可选字段不写，免得带个 null 进配置
        expect('organization' in p).toBe(false);
    });

    it('skips auth for local services and names custom ones generically', () => {
        expect(newMaiBotProvider([], preset('ollama')).auth_type).toBe('none');
        expect(newMaiBotProvider([], preset('custom')).name).toBe('提供商');
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


describe('新增联网服务和分时价格配置', () => {
    it('联网任务引用服务名，模型改名不会改掉同名服务引用', () => {
        const cfg = maibotDefaultConfig(23001);
        const name = cfg.models.models[0]!.name;
        cfg.models.web.services[name] = {
            ...newItemFor(nodeAt(MODEL_SCHEMA, ['web', 'services'])!), api_key: 'key',
        } as NonNullable<typeof cfg.models.web.services[string]>;
        cfg.models.model_task_config.web_search.model_list = [name];
        expect(validateMaiBotConfig(cfg)).toEqual([]);
        cfg.models.models[0]!.name = 'renamed';
        const moved = renameMaiBotModel(cfg.models, name, 'renamed');
        expect(moved.model_task_config.web_search.model_list).toEqual([name]);
        expect(MAIBOT_TASK_KEYS).toContain('image_embedding');
        delete cfg.models.web.services[name];
        expect(validateMaiBotConfig(cfg).map((issue) => issue.path)).toContain('models/model_task_config/web_search/model_list/0');
    });

    it('活动联网服务需要凭据，服务地址拒绝内嵌认证和查询参数', () => {
        const cfg = maibotDefaultConfig(23001);
        cfg.models.web.services.search = {
            ...newItemFor(nodeAt(MODEL_SCHEMA, ['web', 'services'])!), base_url: 'https://user:pass@example.com/?key=x',
        } as NonNullable<typeof cfg.models.web.services[string]>;
        cfg.models.model_task_config.web_search.model_list = ['search'];
        const paths = validateMaiBotConfig(cfg).map((issue) => issue.path);
        expect(paths).toContain('models/web/services/search/base_url');
        expect(paths).toContain('models/web/services/search/api_key');
    });

    it('跨午夜的价格时段不允许重叠，相邻区间可以共用边界', () => {
        const cfg = maibotDefaultConfig(23001);
        cfg.models.models[0]!.price_periods = [
            { start_time: '23:00', end_time: '01:00', price_in: 1, price_out: 2, cache_price_in: 0 },
            { start_time: '00:30', end_time: '02:00', price_in: 1, price_out: 2, cache_price_in: 0 },
        ];
        expect(validateMaiBotConfig(cfg).some((issue) => issue.message.includes('重叠'))).toBe(true);
        cfg.models.models[0]!.price_periods[1]!.start_time = '01:00';
        expect(validateMaiBotConfig(cfg)).toEqual([]);
    });
});
