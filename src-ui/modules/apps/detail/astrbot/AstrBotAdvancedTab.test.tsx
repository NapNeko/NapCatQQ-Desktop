// @vitest-environment jsdom
import { useState } from 'react';
import { act, cleanup, fireEvent, render, screen } from '@testing-library/react';
import userEvent from '@testing-library/user-event';
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from 'vitest';
import { astrbotDefaultConfig } from '../../../../core/domain/apps/astrbotConfig';
import {
    fallbackAstrBotSchema,
    parseAdvanced,
    projectAdvanced,
    readPath,
} from '../../../../core/domain/apps/astrbotAdvanced';
import snapshot from '../../../../core/domain/apps/astrbotSchema/snapshot.json';
import { preferencesStore } from '../../../../core/domain/settings/preferencesStore';
import type { AstrBotInstanceConfig } from '../../../../core/ipc/types';
import type { JsonCodeEditorProps } from '../../../../shared/ui/JsonCodeEditor';
import { AstrBotAdvancedTab } from './AstrBotAdvancedTab';

const { schemaQuery } = vi.hoisted(() => ({ schemaQuery: vi.fn() }));
vi.mock('../../../../hooks/apps/useAstrBotDashboard', () => ({
    useAstrBotConfigSchema: schemaQuery,
}));
vi.mock('../../../../shared/ui/JsonCodeEditor', () => ({
    JsonCodeEditor: ({ ariaLabel, value, readOnly, onChange }: JsonCodeEditorProps) => (
        <textarea
            aria-label={ariaLabel}
            value={value}
            readOnly={readOnly}
            onChange={(event) => onChange(event.currentTarget.value)}
        />
    ),
}));

beforeAll(() => {
    Element.prototype.hasPointerCapture ??= () => false;
    Element.prototype.releasePointerCapture ??= () => {};
    Element.prototype.scrollIntoView ??= () => {};
});

beforeEach(() => {
    act(() => {
        preferencesStore.reset();
        preferencesStore.setMotionEnabled(false);
    });
    schemaQuery.mockReturnValue({ data: undefined, isFetching: false, refetch: vi.fn() });
});

afterEach(() => {
    cleanup();
    act(() => preferencesStore.reset());
});

function defaultConfig(): AstrBotInstanceConfig {
    const config = astrbotDefaultConfig(6199);
    config.advanced_json = JSON.stringify(projectAdvanced(snapshot.defaults));
    config.advanced_base_json = config.advanced_json;
    return config;
}

function mount(initial = defaultConfig(), live = false) {
    const onChange = vi.fn<(next: AstrBotInstanceConfig) => void>();
    const onDraft = vi.fn();
    function Editor() {
        const [config, setConfig] = useState(initial);
        return (
            <AstrBotAdvancedTab
                instanceId="astrbot-test"
                live={live}
                config={config}
                onChange={(next) => {
                    onChange(next);
                    setConfig(next);
                }}
                drafts={{}}
                onDraft={onDraft}
            />
        );
    }
    return { ...render(<Editor />), onChange, onDraft };
}

function search(path: string) {
    fireEvent.change(screen.getByLabelText('搜索高级设置'), { target: { value: path } });
}

describe('AstrBot 高级设置', () => {
    it.each(['fallback', 'live'] as const)(
        '%s 的真实默认配置使用插件选择器，打开页面不会改写配置',
        (source) => {
            const live = source === 'live';
            schemaQuery.mockReturnValue({
                data: live ? fallbackAstrBotSchema() : undefined,
                isFetching: false,
                refetch: vi.fn(),
            });
            const { onChange } = mount(defaultConfig(), live);
            search('plugin_set');

            expect(screen.getByRole('combobox', { name: '可用插件' })).toHaveTextContent(
                '全部已启用插件',
            );
            expect(screen.queryByRole('switch', { name: '可用插件' })).not.toBeInTheDocument();
            expect(screen.queryByText(/plugin_set.*需要布尔值/)).not.toBeInTheDocument();
            expect(screen.queryAllByRole('alert')).toHaveLength(0);
            expect(onChange).not.toHaveBeenCalled();
        },
    );

    it('切换模式、添加和移除插件始终写回字符串数组，空列表保留为不启用', async () => {
        const user = userEvent.setup();
        const { onChange } = mount();
        search('plugin_set');

        await user.click(screen.getByRole('combobox', { name: '可用插件' }));
        await user.click(await screen.findByRole('option', { name: '指定插件' }));
        expect(screen.getByText('留空时不启用任何插件')).toBeVisible();

        await user.type(screen.getByLabelText('插件名称'), 'astrbot_plugin_alpha{Enter}');
        await user.type(screen.getByLabelText('插件名称'), 'astrbot_plugin_beta{Enter}');
        await user.click(screen.getByRole('button', { name: '移除 astrbot_plugin_alpha' }));
        await user.click(screen.getByRole('button', { name: '移除 astrbot_plugin_beta' }));
        expect(screen.getByRole('combobox', { name: '可用插件' })).toHaveTextContent('指定插件');
        expect(screen.getByLabelText('插件名称')).toHaveValue('');

        await user.click(screen.getByRole('combobox', { name: '可用插件' }));
        await user.click(await screen.findByRole('option', { name: '全部已启用插件' }));
        expect(screen.queryByLabelText('插件名称')).not.toBeInTheDocument();
        expect(
            onChange.mock.calls.map(([next]) =>
                readPath(parseAdvanced(next.advanced_json), 'plugin_set'),
            ),
        ).toEqual([
            [],
            ['astrbot_plugin_alpha'],
            ['astrbot_plugin_alpha', 'astrbot_plugin_beta'],
            ['astrbot_plugin_beta'],
            [],
            ['*'],
        ]);
    });

    it('按隐藏配置路径搜索也不会重新暴露其表单字段', () => {
        mount();

        for (const path of [
            't2i_active_template',
            'agent_runner.config.persona.safety_mode_strategy',
            'dashboard.totp',
        ]) {
            search(path);
            expect(screen.getByText('没有匹配的配置项')).toBeVisible();
        }
        expect(screen.queryByLabelText('图片回复模板')).not.toBeInTheDocument();
        expect(screen.queryByRole('switch', { name: /TOTP/ })).not.toBeInTheDocument();
    });

    it('未知特殊控件保留 null，JSON 编辑后也不将其替换为空数组', () => {
        const config = astrbotDefaultConfig(6199);
        config.advanced_json = JSON.stringify({ future_config: null });
        schemaQuery.mockReturnValue({
            data: {
                defaults_json: JSON.stringify({ future_config: null }),
                metadata_json: JSON.stringify({
                    future_group: {
                        metadata: {
                            general: {
                                type: 'object',
                                items: {
                                    future_config: {
                                        description: '未来配置',
                                        type: 'bool',
                                        _special: 'future_unknown_selector',
                                    },
                                },
                            },
                        },
                    },
                }),
            },
            isFetching: false,
            refetch: vi.fn(),
        });
        const { onChange } = mount(config, true);
        search('future_config');
        const editor = screen.getByLabelText('未来配置');

        expect(editor).toHaveValue('null');
        expect(screen.queryByRole('switch', { name: '未来配置' })).not.toBeInTheDocument();
        fireEvent.change(editor, { target: { value: ' null ' } });

        expect(onChange).toHaveBeenCalledTimes(1);
        expect(parseAdvanced(onChange.mock.calls[0]![0].advanced_json)).toEqual({
            future_config: null,
        });
        expect(editor).toHaveValue('null');
    });

    it('完整高级 JSON 隐去陈旧草稿中的 TOTP，重新编辑也不能写回其内部状态', () => {
        const config = defaultConfig();
        const unsafe = {
            ...parseAdvanced(config.advanced_json),
            dashboard: {
                ssl: { enable: false },
                totp: {
                    enable: true,
                    secret: 'fixture-totp-secret',
                    recovery_code_hash: 'fixture-recovery-hash',
                },
            },
        };
        config.advanced_json = JSON.stringify(unsafe);
        const { onChange } = mount(config);
        const editor = screen.getByLabelText<HTMLTextAreaElement>('完整高级配置 JSON');

        expect(readPath(parseAdvanced(editor.value), 'dashboard.totp')).toBeUndefined();
        expect(editor.value).not.toContain('fixture-totp-secret');
        expect(editor.value).not.toContain('fixture-recovery-hash');
        expect(readPath(parseAdvanced(editor.value), 'dashboard.ssl.enable')).toBe(false);

        fireEvent.change(editor, {
            target: { value: JSON.stringify({ ...unsafe, plugin_set: ['astrbot_plugin_raw'] }) },
        });

        expect(onChange).toHaveBeenCalledTimes(1);
        const saved = parseAdvanced(onChange.mock.calls[0]![0].advanced_json);
        expect(readPath(saved, 'dashboard.totp')).toBeUndefined();
        expect(readPath(saved, 'dashboard.ssl.enable')).toBe(false);
        expect(saved.plugin_set).toEqual(['astrbot_plugin_raw']);
    });
});
