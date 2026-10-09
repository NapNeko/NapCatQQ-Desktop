import { expect, it } from 'vitest';
import { defaultsFor, modelAssignments, paramsPath, parseFields } from './neobotWorkspace';
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
