import type { ReactNode } from 'react';
import { FormSection, NumberField, Switch, TextField } from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import { Pill } from './parts';
import type { AstrBotInstanceConfig, AstrBotOneBotRow } from '../../../../core/ipc/types';
import { AstrBotReadyBar } from './AstrBotReadyBar';

export interface AstrBotConnectionsTabProps {
    config: AstrBotInstanceConfig;
    onChange: (next: AstrBotInstanceConfig) => void;
    errors: Record<string, string>;
    linked: boolean;
    disabled?: boolean;
    onGoTab: (tab: string) => void;
    /** 表单之外的即时操作块（WebUI 账号卡） */
    footer?: ReactNode;
}

export const AstrBotConnectionsTab: React.FC<AstrBotConnectionsTabProps> = ({
    config,
    onChange,
    errors,
    linked,
    disabled,
    onGoTab,
    footer,
}) => {
    const row = config.onebot;
    const set = (patch: Partial<AstrBotOneBotRow>) =>
        onChange({ ...config, onebot: { ...row, ...patch } });
    const lockPort = linked || disabled;

    return (
        <ConfigForm>
            <AstrBotReadyBar config={config} onGoTab={onGoTab} />
            <FormSection
                title="OneBot v11"
                actions={
                    <Switch
                        label="启用"
                        checked={row.enable}
                        disabled={lockPort}
                        onCheckedChange={(enable) => set({ enable })}
                    />
                }
            >
                <div className={CONFIG_PAIR}>
                    <TextField label="标识" value={row.id} disabled className="font-mono" />
                    <NumberField label="WebUI 端口" value={config.dashboard_port} disabled />
                    <TextField
                        label="监听地址"
                        value={row.ws_reverse_host}
                        error={errors['onebot/ws_reverse_host']}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(v) => set({ ws_reverse_host: v })}
                    />
                    <NumberField
                        label="监听端口"
                        value={row.ws_reverse_port}
                        min={1}
                        max={65535}
                        error={errors['onebot/ws_reverse_port']}
                        disabled={lockPort}
                        hint={linked ? '已对接，改口会一并重写协议端的连接地址' : undefined}
                        onValueChange={(v) => set({ ws_reverse_port: v ?? row.ws_reverse_port })}
                    />
                    <TextField
                        label="连接密钥"
                        value={row.ws_reverse_token}
                        disabled={disabled}
                        className="font-mono sm:col-span-2"
                        onValueChange={(v) => set({ ws_reverse_token: v })}
                    />
                </div>
                {!config.claimed && (
                    <p className="text-xs text-text-tertiary">
                        还没有认领的 OneBot v11 连接，对接协议端时会自动加一条
                    </p>
                )}
            </FormSection>
            {config.other_platforms.length > 0 && (
                <FormSection title="其它平台" description="在 AstrBot WebUI 里管" layout="none">
                    <div className="flex flex-wrap gap-1.5">
                        {config.other_platforms.map((p) => (
                            <Pill key={p}>{p}</Pill>
                        ))}
                    </div>
                </FormSection>
            )}
            {footer}
        </ConfigForm>
    );
};
