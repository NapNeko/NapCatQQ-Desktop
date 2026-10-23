import { describe, expect, it } from 'vitest';
import { astrbotDefaultConfig, astrbotSetup } from './astrbotConfig';
import snapshot from './astrbotSchema/snapshot.json';
import {
    advancedSections,
    advancedValueIssues,
    effectiveAdvancedRoot,
    fallbackAstrBotSchema,
    isManagedPath,
    parseAdvanced,
    projectAdvanced,
    readPath,
    resolveAdvancedSchema,
    setAdvancedField,
} from './astrbotAdvanced';

describe('AstrBot advanced config coverage', () => {
    it.each(['fallback', 'live'] as const)(
        'accepts the full upstream defaults through the %s metadata path',
        (source) => {
            const cfg = astrbotDefaultConfig(6199);
            cfg.advanced_json = JSON.stringify(projectAdvanced(snapshot.defaults));
            const schema = resolveAdvancedSchema(
                source === 'live' ? fallbackAstrBotSchema() : undefined,
            );
            const sections = advancedSections(cfg, schema);
            expect(schema.live).toBe(source === 'live');
            expect(advancedValueIssues(cfg, sections)).toEqual([]);
            expect(readPath(effectiveAdvancedRoot(cfg, schema), 'plugin_set')).toEqual(['*']);
            const plugin = sections
                .flatMap((section) => section.fields)
                .find((field) => field.path === 'plugin_set');
            expect(plugin?.metadata).toMatchObject({ type: 'list', items: { type: 'string' } });
        },
    );

    it.each([
        { name: 'all', selection: ['*'] },
        { name: 'none', selection: [] },
        { name: 'custom', selection: ['enabled_plugin', 'disabled_plugin', 'unknown_plugin'] },
    ])('round-trips $name plugins without losing hidden or unknown values', ({ selection }) => {
        const cfg = astrbotDefaultConfig(6199);
        cfg.advanced_json = JSON.stringify({
            ...projectAdvanced(snapshot.defaults),
            t2i_active_template: 'upstream-hidden-template',
            future_settings: { upstream_only: { preserve: true } },
        });
        const schema = resolveAdvancedSchema();
        const field = advancedSections(cfg, schema)
            .flatMap((section) => section.fields)
            .find((item) => item.path === 'plugin_set');
        expect(field).toBeDefined();
        const changed = setAdvancedField(cfg, field!, selection);
        const reread = { ...cfg, advanced_json: changed.advanced_json };
        const root = effectiveAdvancedRoot(reread, schema);
        expect(readPath(parseAdvanced(changed.advanced_json), 'plugin_set')).toEqual(selection);
        expect(readPath(root, 'plugin_set')).toEqual(selection);
        expect(readPath(root, 't2i_active_template')).toBe('upstream-hidden-template');
        expect(readPath(root, 'future_settings.upstream_only')).toEqual({ preserve: true });
        expect(advancedValueIssues(reread, advancedSections(reread, schema))).toEqual([]);
    });

    it.each([
        { name: 'true', value: true },
        { name: 'false', value: false },
        { name: 'string', value: 'one_plugin' },
        { name: 'number member', value: ['one_plugin', 1] },
        { name: 'null member', value: ['one_plugin', null] },
        { name: 'object member', value: ['one_plugin', { name: 'other' }] },
    ])('rejects an invalid plugin selection: $name', ({ value }) => {
        const cfg = astrbotDefaultConfig(6199);
        cfg.advanced_json = JSON.stringify({ plugin_set: value });
        const issues = advancedValueIssues(cfg, advancedSections(cfg, resolveAdvancedSchema()));
        expect(issues.some((issue) => issue.path === 'advanced/plugin_set')).toBe(true);
    });

    it.each([
        { name: 'number', value: 37, invalid: true },
        { name: 'array', value: ['value'], invalid: true },
        { name: 'null', value: null, invalid: true },
        { name: 'dictionary', value: { upstream_variable: 'value' }, invalid: false },
    ])('validates an upstream dict as an object: $name', ({ value, invalid }) => {
        const cfg = astrbotDefaultConfig(6199);
        cfg.ai.agent_runner_type = 'dify';
        cfg.advanced_json = JSON.stringify({
            agent_runner: { runner_type: 'dify', config: { variables: value } },
        });
        const sections = advancedSections(cfg, resolveAdvancedSchema());
        const field = sections
            .flatMap((section) => section.fields)
            .find((item) => item.path === 'agent_runner.config.variables');
        expect(field?.metadata.type).toBe('object');
        expect(
            advancedValueIssues(cfg, sections).some(
                (issue) => issue.path === 'advanced/agent_runner.config.variables',
            ),
        ).toBe(invalid);
    });

    it('does not rediscover invisible fields or children of hidden parents and sections', () => {
        const values = {
            visible: false,
            hidden_leaf: true,
            hidden_parent: { child: true },
            parent: { hidden_child: { flag: true } },
            hidden_section: { flag: true },
        };
        const cfg = astrbotDefaultConfig(6199);
        cfg.advanced_json = JSON.stringify(values);
        const schema = resolveAdvancedSchema({
            defaults_json: JSON.stringify(values),
            metadata_json: JSON.stringify({
                future_group: {
                    metadata: {
                        general: {
                            type: 'object',
                            items: {
                                visible: { type: 'bool' },
                                hidden_leaf: { type: 'bool', invisible: true },
                                hidden_parent: {
                                    type: 'object',
                                    invisible: true,
                                    items: { child: { type: 'bool' } },
                                },
                                parent: {
                                    type: 'object',
                                    items: {
                                        hidden_child: {
                                            type: 'object',
                                            invisible: true,
                                            items: { flag: { type: 'bool' } },
                                        },
                                    },
                                },
                            },
                        },
                        hidden: {
                            type: 'object',
                            invisible: true,
                            items: { 'hidden_section.flag': { type: 'bool' } },
                        },
                    },
                },
            }),
        });
        const fields = advancedSections(cfg, schema).flatMap((section) => section.fields);
        const hiddenPaths = [
            'hidden_leaf',
            'hidden_parent',
            'parent.hidden_child',
            'hidden_section',
        ];
        expect(
            fields.some((field) =>
                hiddenPaths.some(
                    (path) => field.path === path || field.path.startsWith(path + '.'),
                ),
            ),
        ).toBe(false);
        const visible = fields.find((field) => field.path === 'visible');
        expect(visible).toBeDefined();
        const changed = setAdvancedField(cfg, visible!, true);
        expect(parseAdvanced(changed.advanced_json)).toEqual({ ...values, visible: true });
    });

    it('keeps unknown special controls editable as JSON without boolean validation', () => {
        const cfg = astrbotDefaultConfig(6199);
        cfg.advanced_json = JSON.stringify({ future: { selection: ['hidden_plugin'] } });
        const schema = resolveAdvancedSchema({
            defaults_json: JSON.stringify({ future: { selection: ['default_plugin'] } }),
            metadata_json: JSON.stringify({
                future_group: {
                    metadata: {
                        future: {
                            type: 'object',
                            items: {
                                'future.selection': {
                                    type: 'bool',
                                    _special: 'future_plugin_selector',
                                    options: [true, false],
                                    items: { type: 'bool' },
                                },
                            },
                        },
                    },
                },
            }),
        });
        const sections = advancedSections(cfg, schema);
        const field = sections.flatMap((section) => section.fields)[0];
        expect(field?.path).toBe('future.selection');
        expect(field?.metadata.type).toBe('json');
        expect(field?.metadata.options).toBeUndefined();
        expect(field?.metadata.items).toBeUndefined();
        expect(advancedValueIssues(cfg, sections)).toEqual([]);
        const changed = setAdvancedField(cfg, field!, ['hidden_plugin', 'unknown_plugin']);
        expect(readPath(parseAdvanced(changed.advanced_json), 'future.selection')).toEqual([
            'hidden_plugin',
            'unknown_plugin',
        ]);
        expect(advancedValueIssues(changed, advancedSections(changed, schema))).toEqual([]);
    });

    it('excludes action controls and TOTP secrets while preserving plugins during another edit', () => {
        const root = {
            plugin_set: ['disabled_plugin', 'unknown_plugin'],
            t2i_template: true,
            t2i_active_template: 'hidden-template',
            dashboard: {
                totp: {
                    enable: true,
                    secret: 'synthetic-test-secret',
                    recovery_code_hash: 'synthetic-test-recovery-hash',
                },
                ssl: { enable: true },
            },
            provider_tts_settings: { use_file_service: false },
        };
        const projected = projectAdvanced(root);
        expect(readPath(projected, 'dashboard.totp')).toBeUndefined();
        expect(readPath(projected, 'dashboard.ssl.enable')).toBe(true);
        const raw = JSON.stringify(projected);
        expect(raw).not.toContain('synthetic-test-secret');
        expect(raw).not.toContain('synthetic-test-recovery-hash');
        const cfg = astrbotDefaultConfig(6199);
        cfg.advanced_json = JSON.stringify(root);
        const schema = resolveAdvancedSchema();
        const fields = advancedSections(cfg, schema).flatMap((section) => section.fields);
        expect(fields.some((field) => field.path === 't2i_template')).toBe(false);
        expect(fields.some((field) => field.path === 't2i_active_template')).toBe(false);
        expect(fields.some((field) => field.path.startsWith('dashboard.totp'))).toBe(false);
        const field = fields.find((item) => item.path === 'provider_tts_settings.use_file_service');
        expect(field).toBeDefined();
        const changed = setAdvancedField(cfg, field!, true);
        const reread = parseAdvanced(changed.advanced_json);
        expect(readPath(reread, 'plugin_set')).toEqual(root.plugin_set);
        expect(readPath(reread, 'dashboard.totp')).toBeUndefined();
        expect(readPath(reread, 't2i_active_template')).toBe('hidden-template');
        expect(advancedValueIssues(changed, advancedSections(changed, schema))).toEqual([]);
    });

    it('discovers current metadata and default-only settings without duplicating dedicated controls', () => {
        const cfg = astrbotDefaultConfig(6199);
        const schema = resolveAdvancedSchema();
        const fields = advancedSections(cfg, schema).flatMap((section) => section.fields);
        const paths = fields.map((field) => field.path);
        expect(paths).toContain('provider_settings.websearch_brave_key');
        expect(paths).toContain('provider_settings.websearch_serpapi_key');
        expect(paths).toContain('provider_tts_settings.use_file_service');
        expect(paths).toContain('platform_settings.segmented_reply.enable');
        expect(paths).toContain('agent_runner.config.compression.keep_recent_ratio');
        expect(paths).not.toContain('provider_settings.enable');
        expect(paths).not.toContain('agent_runner.config.model.provider_id');
        expect(paths).not.toContain('dashboard.password');
        expect(fields.every((field) => !isManagedPath(field.path))).toBe(true);
    });

    it('does not require a local model for an external Agent Runner', () => {
        const cfg = astrbotDefaultConfig(6199);
        cfg.ai.agent_runner_type = 'dify';
        expect(astrbotSetup(cfg, true).ready).toBe(true);
        cfg.ai.enable = false;
        expect(astrbotSetup(cfg, true).llmIssue).toBe('llm_off');
    });

    it('accepts new instance metadata and values without a client release', () => {
        const cfg = astrbotDefaultConfig(6199);
        cfg.advanced_json = JSON.stringify({ future: { enabled: true, limit: 5 } });
        const schema = resolveAdvancedSchema({
            defaults_json: JSON.stringify({ future: { enabled: false, limit: 3 } }),
            metadata_json: JSON.stringify({
                future_group: {
                    name: '新能力',
                    metadata: {
                        future: {
                            description: '新配置',
                            type: 'object',
                            items: {
                                'future.enabled': { type: 'bool', description: '启用新能力' },
                            },
                        },
                    },
                },
            }),
        });
        const fields = advancedSections(cfg, schema).flatMap((section) => section.fields);
        expect(fields.find((field) => field.path === 'future.enabled')?.label).toBe('启用新能力');
        expect(fields.some((field) => field.path === 'future.limit')).toBe(true);
        expect(readPath(effectiveAdvancedRoot(cfg, schema), 'future.enabled')).toBe(true);
    });

    it('filters dedicated ownership and rejects prototype traversal', () => {
        const root = projectAdvanced({
            platform: [{ id: 'owned' }],
            provider_settings: { enable: false, websearch_exa_key: ['key'] },
            dashboard: { port: 6185, password: 'secret', ssl: { enable: true } },
        });
        expect(readPath(root, 'platform')).toBeUndefined();
        expect(readPath(root, 'provider_settings.enable')).toBeUndefined();
        expect(readPath(root, 'provider_settings.websearch_exa_key')).toEqual(['key']);
        expect(readPath(root, 'dashboard.password')).toBeUndefined();
        expect(readPath(root, 'dashboard.ssl.enable')).toBe(true);
        expect(readPath({}, '__proto__.polluted')).toBeUndefined();
    });

    it('does not let non-object ancestors pass dedicated ownership filtering', () => {
        expect(projectAdvanced({ dashboard: null, provider_settings: [] })).toEqual({});
        const projected = projectAdvanced({ agent_runner: { config: { model: null } } });
        expect(readPath(projected, 'agent_runner.config.model')).toBeUndefined();
    });

    it('switches runners using the selected upstream defaults and keeps dedicated fields out of the draft', () => {
        const cfg = astrbotDefaultConfig(6199);
        cfg.advanced_json = JSON.stringify({
            agent_runner: {
                runner_type: 'local',
                config: { model: { request_max_retries: 7 }, misc: { future: true } },
            },
        });
        const field = advancedSections(cfg, resolveAdvancedSchema())
            .flatMap((section) => section.fields)
            .find((item) => item.path === 'agent_runner.runner_type');
        expect(field).toBeDefined();
        const changed = setAdvancedField(cfg, field!, 'dify');
        const advanced: unknown = JSON.parse(changed.advanced_json);
        expect(readPath(advanced, 'agent_runner.runner_type')).toBe('dify');
        expect(readPath(advanced, 'agent_runner.config.dify_api_type')).toBe('chat');
        expect(readPath(advanced, 'agent_runner.config.model')).toBeUndefined();
        expect(readPath(advanced, 'agent_runner.config.misc.future')).toBeUndefined();
    });

    it('reports invalid primitive types and upstream numeric ranges', () => {
        const cfg = astrbotDefaultConfig(6199);
        cfg.advanced_json = JSON.stringify({
            provider_tts_settings: { use_file_service: 'invalid' },
            agent_runner: {
                runner_type: 'local',
                config: { compression: { keep_recent_ratio: 0.9 } },
            },
        });
        const issues = advancedValueIssues(cfg, advancedSections(cfg, resolveAdvancedSchema()));
        expect(
            issues.some(
                (issue) => issue.path === 'advanced/provider_tts_settings.use_file_service',
            ),
        ).toBe(true);
        expect(
            issues.some(
                (issue) =>
                    issue.path === 'advanced/agent_runner.config.compression.keep_recent_ratio',
            ),
        ).toBe(true);
    });
});
