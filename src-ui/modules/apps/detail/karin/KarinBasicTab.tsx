// Karin「基础」：进程怎么跑。听口 / 对接口在「连接」，不在这里重复 HTTP。

import { FormSection, KeyValueListEditor, NumberField, Select, Switch, TextField } from '../../../../shared/ui';
import { KARIN_LOG_LEVELS, KARIN_NODE_ENVS, KARIN_RUNTIMES } from '../../../../core/domain/apps/karinConfig';
import { CONFIG_PAIR, ConfigForm } from './configLayout';
import type { KarinEnv, KarinInstanceConfig } from '../../../../core/ipc/types';

export interface KarinTabProps {
    config: KarinInstanceConfig;
    onChange: (next: KarinInstanceConfig) => void;
    errors: Record<string, string>;
    linked: boolean;
    disabled?: boolean;
}

const LOG_LEVEL_ITEMS = KARIN_LOG_LEVELS.map((v) => ({ value: v, label: v }));
const RUNTIME_ITEMS = KARIN_RUNTIMES.map((v) => ({ value: v, label: v }));
const NODE_ENV_ITEMS = KARIN_NODE_ENVS.map((v) => ({ value: v, label: v }));

export const KarinBasicTab: React.FC<KarinTabProps> = ({ config, onChange, errors, disabled }) => {
    const env = config.env;
    const set = (patch: Partial<KarinEnv>) => onChange({ ...config, env: { ...env, ...patch } });

    const keyErrors: Record<number, string | undefined> = {};
    env.custom.forEach((_, i) => {
        keyErrors[i] = errors[`env/custom/${i}/key`];
    });

    return (
        <ConfigForm>
            <FormSection title="运行时">
                <div className={CONFIG_PAIR}>
                    <Select
                        label="运行器"
                        error={errors['env/runtime']}
                        items={RUNTIME_ITEMS}
                        value={env.runtime}
                        disabled={disabled}
                        onValueChange={(v) => set({ runtime: v })}
                    />
                    <Select
                        label="NODE_ENV"
                        error={errors['env/node_env']}
                        items={NODE_ENV_ITEMS}
                        value={env.node_env}
                        disabled={disabled}
                        onValueChange={(v) => set({ node_env: v })}
                    />
                </div>
                <div className="flex flex-wrap gap-x-8 gap-y-3">
                    <Switch
                        label="启用 Redis"
                        checked={env.redis_enable}
                        disabled={disabled}
                        onCheckedChange={(v) => set({ redis_enable: v })}
                    />
                    <Switch
                        label="重启走 pm2"
                        checked={env.pm2_restart}
                        disabled={disabled}
                        onCheckedChange={(v) => set({ pm2_restart: v })}
                    />
                    <Switch
                        label="tsx watch"
                        checked={env.tsx_watch}
                        disabled={disabled}
                        onCheckedChange={(v) => set({ tsx_watch: v })}
                    />
                </div>
            </FormSection>

            <FormSection title="日志">
                <div className={CONFIG_PAIR}>
                    <Select
                        label="等级"
                        error={errors['env/log_level']}
                        items={LOG_LEVEL_ITEMS}
                        value={env.log_level}
                        disabled={disabled}
                        onValueChange={(v) => set({ log_level: v })}
                    />
                    <NumberField
                        label="保留天数"
                        value={env.log_days_to_keep}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => set({ log_days_to_keep: v ?? 0 })}
                    />
                    <NumberField
                        label="单文件上限（MB）"
                        hint="0 不分割"
                        value={env.log_max_log_size}
                        min={0}
                        disabled={disabled}
                        onValueChange={(v) => set({ log_max_log_size: v ?? 0 })}
                    />
                    <NumberField
                        label="实时日志连接数"
                        value={env.log_max_connections}
                        min={1}
                        disabled={disabled}
                        onValueChange={(v) => set({ log_max_connections: v ?? 1 })}
                    />
                </div>
                <div className="flex max-w-xs items-end gap-2">
                    <TextField
                        label="fnc 颜色"
                        value={env.log_fnc_color}
                        disabled={disabled}
                        className="min-w-0 flex-1 font-mono"
                        onValueChange={(v) => set({ log_fnc_color: v })}
                    />
                    <span
                        aria-hidden
                        className="mb-0.5 h-9 w-9 shrink-0 rounded-sm border border-border-subtle"
                        style={{ background: env.log_fnc_color || 'transparent' }}
                    />
                </div>
            </FormSection>

            <FormSection title="FFmpeg" description="留空走 PATH">
                <TextField
                    label="ffmpeg"
                    value={env.ffmpeg_path}
                    disabled={disabled}
                    className="font-mono"
                    onValueChange={(v) => set({ ffmpeg_path: v })}
                />
                <TextField
                    label="ffprobe"
                    value={env.ffprobe_path}
                    disabled={disabled}
                    className="font-mono"
                    onValueChange={(v) => set({ ffprobe_path: v })}
                />
                <TextField
                    label="ffplay"
                    value={env.ffplay_path}
                    disabled={disabled}
                    className="font-mono"
                    onValueChange={(v) => set({ ffplay_path: v })}
                />
            </FormSection>

            <FormSection title="自定义键">
                <KeyValueListEditor
                    value={env.custom}
                    keyErrors={keyErrors}
                    disabled={disabled}
                    addLabel="添加"
                    onChange={(custom) => set({ custom })}
                />
            </FormSection>
        </ConfigForm>
    );
};
