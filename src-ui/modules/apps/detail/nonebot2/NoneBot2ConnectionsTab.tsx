import {
    FormSection,
    KeyValueListEditor,
    NumberField,
    SecretField,
    Select,
    Switch,
    TextField,
    StringListField,
} from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import {
    NONEBOT2_ENV_PRESETS,
    NONEBOT2_LOG_LEVELS,
    appendNoneBot2EnvPreset,
} from '../../../../core/domain/apps/nonebot2Config';
import type { NoneBot2EnvProd, NoneBot2InstanceConfig } from '../../../../core/ipc/types';

export interface NoneBot2TabProps {
    config: NoneBot2InstanceConfig;
    onChange: (next: NoneBot2InstanceConfig) => void;
    errors: Record<string, string>;
    linked: boolean;
    disabled?: boolean;
}

export const NoneBot2ConnectionsTab: React.FC<NoneBot2TabProps> = ({
    config,
    onChange,
    errors,
    linked,
    disabled,
}) => {
    const env = config.env_prod;
    const set = (patch: Partial<NoneBot2EnvProd>) =>
        onChange({ ...config, env_prod: { ...env, ...patch } });

    const allowBare = env.command_start.includes('');
    const prefixes = env.command_start.filter((s) => s !== '');

    const keyErrors: Record<number, string | undefined> = {};
    env.custom.forEach((_, i) => {
        keyErrors[i] = errors[`env_prod/custom/${i}/key`];
    });

    return (
        <ConfigForm>
            <FormSection title="对接">
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="HOST"
                        value={env.host}
                        error={errors['env_prod/host']}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(v) => set({ host: v })}
                    />
                    <NumberField
                        label="PORT"
                        value={env.port}
                        min={1}
                        max={65535}
                        error={errors['env_prod/port']}
                        disabled={disabled}
                        hint={linked ? '改端口或 token 会重写对接' : undefined}
                        onValueChange={(v) => set({ port: v ?? env.port })}
                    />
                    <SecretField
                        label="ONEBOT_ACCESS_TOKEN"
                        value={env.onebot_access_token}
                        error={errors['env_prod/onebot_access_token']}
                        hint="也支持已有 ONEBOT_V11_ACCESS_TOKEN"
                        disabled={disabled}
                        className="font-mono sm:col-span-2"
                        onValueChange={(v) => set({ onebot_access_token: v })}
                    />
                </div>
                <SecretField
                    label="ONEBOT_SECRET"
                    value={env.onebot_secret}
                    error={errors['env_prod/onebot_secret']}
                    hint="HTTP 上报数据的签名口令"
                    disabled={disabled}
                    onValueChange={(onebot_secret) => set({ onebot_secret })}
                />
                <StringListField
                    label="ONEBOT_WS_URLS"
                    value={env.onebot_ws_urls}
                    error={
                        Object.entries(errors).find(([key]) =>
                            key.startsWith('env_prod/onebot_ws_urls/'),
                        )?.[1]
                    }
                    hint="正向 WebSocket 地址；需使用支持客户端连接的驱动"
                    placeholder="ws://127.0.0.1:3001"
                    disabled={disabled}
                    onChange={(onebot_ws_urls) => set({ onebot_ws_urls })}
                />
                <TextField
                    label="ONEBOT_API_ROOTS"
                    value={env.onebot_api_roots}
                    error={errors['env_prod/onebot_api_roots']}
                    hint={'HTTP API 地址，格式：{"10001":"http://127.0.0.1:3000"}'}
                    disabled={disabled}
                    className="font-mono"
                    onValueChange={(onebot_api_roots) => set({ onebot_api_roots })}
                />
            </FormSection>

            <FormSection title="运行">
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="日志等级"
                        hint={`${NONEBOT2_LOG_LEVELS.join(' / ')} 或数值等级`}
                        error={errors['env_prod/log_level']}
                        value={env.log_level}
                        disabled={disabled}
                        onValueChange={(v) => set({ log_level: v })}
                    />
                    <TextField
                        label="DRIVER"
                        value={env.driver}
                        error={errors['env_prod/driver']}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(v) => set({ driver: v })}
                    />
                    <TextField
                        label="API_TIMEOUT"
                        value={env.api_timeout}
                        error={errors['env_prod/api_timeout']}
                        hint="API 请求超时（秒），默认 30"
                        disabled={disabled}
                        onValueChange={(api_timeout) => set({ api_timeout })}
                    />
                    <TextField
                        label="SESSION_EXPIRE_TIMEOUT"
                        value={env.session_expire_timeout}
                        error={errors['env_prod/session_expire_timeout']}
                        hint="等待回复的超时；支持秒数、HH:MM:SS、ISO 时长，负数请用秒数或 -P 开头的 ISO"
                        disabled={disabled}
                        onValueChange={(session_expire_timeout) => set({ session_expire_timeout })}
                    />
                </div>
                <StringListField
                    label="SUPERUSERS"
                    value={env.superusers}
                    disabled={disabled}
                    onChange={(superusers) => set({ superusers })}
                />
                <StringListField
                    label="NICKNAME"
                    value={env.nickname}
                    disabled={disabled}
                    onChange={(nickname) => set({ nickname })}
                />
                <StringListField
                    label="命令前缀"
                    value={prefixes}
                    disabled={disabled}
                    onChange={(next) => set({ command_start: allowBare ? [...next, ''] : next })}
                />
                <Switch
                    label="无前缀也响应"
                    checked={allowBare}
                    disabled={disabled}
                    onCheckedChange={(v) =>
                        set({
                            command_start: v ? [...prefixes, ''] : prefixes,
                        })
                    }
                />
                <StringListField
                    label="命令分隔"
                    value={env.command_sep}
                    disabled={disabled}
                    onChange={(command_sep) => set({ command_sep })}
                />
            </FormSection>

            <FormSection title="驱动与自定义键">
                <Select
                    label="添加上游配置项"
                    placeholder="选择驱动或 OneBot v12 配置"
                    value=""
                    items={NONEBOT2_ENV_PRESETS.filter(
                        (preset) =>
                            !env.custom.some((entry) => entry.key.toUpperCase() === preset.key),
                    ).map((preset) => ({ value: preset.key, label: preset.key }))}
                    disabled={disabled}
                    onValueChange={(key) =>
                        set({ custom: appendNoneBot2EnvPreset(env.custom, key) })
                    }
                />
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
