import { describe, expect, it } from 'vitest';
import { astrbotDefaultConfig, astrbotSetup } from './astrbotConfig';
import {
    advancedSections,
    advancedValueIssues,
    effectiveAdvancedRoot,
    isManagedPath,
    projectAdvanced,
    readPath,
    resolveAdvancedSchema,
    setAdvancedField,
} from './astrbotAdvanced';

describe('AstrBot advanced config coverage', () => {
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
