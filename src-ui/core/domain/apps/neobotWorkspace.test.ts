import { expect, it } from 'vitest';
import {
    defaultsFor,
    modelAssignments,
    modelParamApplies,
    modelParamsValue,
    paramsPath,
    parseFields,
} from './neobotWorkspace';
import { parseNeoBotArchives, parseNeoBotModels } from './neobotPanels';

it('兼容上游嵌套模型分配、平台数组和超限字段', () => {
    const assignments = {
        roles: { chat_model: 'chat/main' },
        creator_image_models: ['images/main'],
    };
    expect(modelAssignments(assignments)).toEqual({
        chat_model: 'chat/main',
        creator_image_models: ['images/main'],
    });
    expect(
        parseNeoBotModels({
            assignments,
            platforms: [{ name: 'OpenAI', has_key: true }],
            library: [{ model_ref: 'chat/main', provider: 'OpenAI' }],
        }),
    ).toMatchObject({
        assignments: { chat_model: 'chat/main' },
        library: [{ providerHasKey: true }],
    });
    expect(
        parseNeoBotArchives({ items: [{ table_name: 'memory', over_limit_count: 5 }] })?.tables[0]
            ?.overLimit,
    ).toBe(5);
});

it('默认草稿不生成密钥值，并安全编码自由文本查询', () => {
    const fields = parseFields([
        {
            name: 'config',
            kind: 'group',
            fields: [
                { name: 'api_key', sensitive: true, default: 'placeholder' },
                { name: 'retries', default: 2 },
            ],
        },
    ]);
    expect(defaultsFor(fields)).toEqual({ config: { retries: 2 } });
    const url = new URL(
        paramsPath('/api/archives/item', { key: '群/1?中文&..', table: 'memory' }),
        'http://localhost',
    );
    expect(url.searchParams.get('key')).toBe('群/1?中文&..');
    expect(url.searchParams.get('table')).toBe('memory');
});

it('已有模型参数以 settings 值为准，保留自定义请求体与未知启用项', () => {
    const [field] = parseFields([
        {
            name: 'params',
            kind: 'model_params',
            value: {
                enabled_params: [],
                extra_body: {},
                values: { deepseek_reasoning_effort: 'high' },
            },
            catalog: [
                {
                    name: 'deepseek_reasoning_effort',
                    label: '思考强度',
                    type: 'str',
                    default: 'high',
                    options: ['high', 'max'],
                    scope: { providers: ['deepseek'], model_types: ['chat', 'vision'] },
                },
            ],
        },
    ]);
    const settings = {
        enabled_params: ['deepseek_reasoning_effort', 'future_option'],
        deepseek_reasoning_effort: 'max',
        extra_body: { vendor_extension: { enabled: true } },
        future_setting: 7,
    };
    expect(modelParamsValue(field!, settings)).toEqual({
        enabled_params: ['deepseek_reasoning_effort', 'future_option'],
        extra_body: { vendor_extension: { enabled: true } },
        values: { deepseek_reasoning_effort: 'max' },
    });
    expect(settings.future_setting).toBe(7);
    const draft = { enabled_params: [], extra_body: {}, values: {} };
    expect(modelParamsValue(field!, { ...settings, params: draft })).toBe(draft);
    expect(field!.catalog[0]).toMatchObject({
        name: 'deepseek_reasoning_effort',
        strict: true,
        options: ['high', 'max'],
    });
    const parameter = field!.catalog[0]!;
    expect(
        modelParamApplies(parameter, { provider: 'DeepSeek-Official', model_type: 'CHAT' }),
    ).toBe(true);
    expect(modelParamApplies(parameter, { provider: 'OpenAI', model_type: 'chat' })).toBe(false);
    expect(modelParamApplies(parameter, { provider: 'DeepSeek', model_type: 'image' })).toBe(false);
});
