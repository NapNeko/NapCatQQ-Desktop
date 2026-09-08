import {
    FormSection,
    KeyValueListEditor,
    NumberField,
    Select,
    Switch,
    TextField,
    StringListField,
} from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import { NONEBOT2_LOG_LEVELS } from '../../../../core/domain/apps/nonebot2Config';
import type { NoneBot2EnvProd, NoneBot2InstanceConfig } from '../../../../core/ipc/types';

export interface NoneBot2TabProps {
    config: NoneBot2InstanceConfig;
    onChange: (next: NoneBot2InstanceConfig) => void;
    errors: Record<string, string>;
    linked: boolean;
    disabled?: boolean;
}

const LOG_ITEMS = NONEBOT2_LOG_LEVELS.map((v) => ({ value: v, label: v }));

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
                    <TextField
                        label="ONEBOT_ACCESS_TOKEN"
                        value={env.onebot_access_token}
                        disabled={disabled}
                        className="font-mono sm:col-span-2"
                        onValueChange={(v) => set({ onebot_access_token: v })}
                    />
                </div>
            </FormSection>

            <FormSection title="运行">
                <div className={CONFIG_PAIR}>
                    <Select
                        label="日志等级"
                        items={LOG_ITEMS}
                        value={env.log_level}
                        disabled={disabled}
                        onValueChange={(v) => set({ log_level: v })}
                    />
                    <TextField
                        label="DRIVER"
                        value={env.driver}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(v) => set({ driver: v })}
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
                    onChange={(next) =>
                        set({ command_start: allowBare ? [...next, ''] : next })
                    }
                />
                <Switch
                    label="无前缀也响应"
                    checked={allowBare}
                    disabled={disabled}
                    onCheckedChange={(v) =>
                        set({
                            command_start: v
                                ? [...prefixes, '']
                                : prefixes,
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
