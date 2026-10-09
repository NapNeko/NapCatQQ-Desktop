import { expect, it } from 'vitest';
import { mockNeoBotWorkspace } from './neobot-workspace';
import { record, records, text } from '../../../domain/apps/neobotWorkspace';

it('预览配置可往返，旧 revision 拒绝覆盖', () => {
    const id = 'test-config';
    const initial = record(mockNeoBotWorkspace(id, 'GET', '/api/config')?.data);
    const config = { ...record(initial.config), desktop_test: true };
    const saved = mockNeoBotWorkspace(id, 'POST', '/api/config', {
        mode: 'form',
        revision: initial.revision,
        config,
        reload: true,
    });
    expect(saved?.kind).toBe('ok');
    expect(record(mockNeoBotWorkspace(id, 'GET', '/api/config')?.data).config).toEqual(config);
    expect(
        mockNeoBotWorkspace(id, 'POST', '/api/config', { revision: initial.revision, config: {} }),
    ).toMatchObject({ kind: 'failed', status: 409 });
});

it('档案修改携带版本，冲突返回当前全文，删除之后不可继续读取', () => {
    const id = 'test-archives';
    const list = record(mockNeoBotWorkspace(id, 'GET', '/api/archives/items')?.data);
    const item = records(list.items)[0];
    const body = {
        table: item.table_name,
        key: item.key,
        version: item.version,
        value: 'new memory',
        tags: ['desktop'],
    };
    const saved = record(mockNeoBotWorkspace(id, 'PUT', '/api/archives/item', body)?.data);
    expect(saved.value).toBe('new memory');
    expect(mockNeoBotWorkspace(id, 'PUT', '/api/archives/item', body)).toMatchObject({
        status: 409,
        data: { current: { value: 'new memory' } },
    });
    expect(
        mockNeoBotWorkspace(id, 'DELETE', '/api/archives/item', { ...body, version: saved.version })
            ?.kind,
    ).toBe('ok');
    expect(
        mockNeoBotWorkspace(
            id,
            'GET',
            `/api/archives/item?key=${encodeURIComponent(text(item.key))}`,
        ),
    ).toMatchObject({ kind: 'failed', status: 404 });
});

it('密钥空值保留，预览响应不回传新密钥原文', () => {
    const id = 'test-env';
    const initial = record(mockNeoBotWorkspace(id, 'GET', '/api/config/env')?.data);
    const saved = record(
        mockNeoBotWorkspace(id, 'POST', '/api/config/env', {
            revision: initial.revision,
            updates: { TEST_API_KEY: 'mock-secret' },
            deletes: [],
        })?.data,
    );
    const blank = record(
        mockNeoBotWorkspace(id, 'POST', '/api/config/env', {
            revision: saved.revision,
            updates: { TEST_API_KEY: '' },
            deletes: [],
        })?.data,
    );
    expect(records(blank.items).find((item) => item.key === 'TEST_API_KEY')).toMatchObject({
        value: '',
        has_value: true,
        sensitive: true,
    });
    expect(JSON.stringify(blank)).not.toContain('mock-secret');
});
