import type { ReactNode } from 'react';
import { FormSection, NumberField, Switch, TextField } from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import type { AstrBotInstanceConfig, AstrBotOneBotRow } from '../../../../core/ipc/types';

export interface AstrBotConnectionsTabProps {
    config: AstrBotInstanceConfig;
    onChange: (next: AstrBotInstanceConfig) => void;
    errors: Record<string, string>;
    linked: boolean;
    disabled?: boolean;
    /** 表单之外的即时操作块（WebUI 账号卡） */
    footer?: ReactNode;
}

export const AstrBotConnectionsTab: React.FC<AstrBotConnectionsTabProps> = ({
    config,
    onChange,
    errors,
    linked,
    disabled,
    footer,
}) => {
    const row = config.onebot;
    const set = (patch: Partial<AstrBotOneBotRow>) =>
        onChange({ ...config, onebot: { ...row, ...patch } });
    const lockPort = linked || disabled;

    return (
        <ConfigForm>
            <FormSection title="OneBot v11">
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="id"
                        value={row.id}
                        disabled
                        className="font-mono"
                    />
                    <NumberField
                        label="WebUI 口"
                        value={config.dashboard_port}
                        disabled
                    />
                    <TextField
                        label="ws_reverse_host"
                        value={row.ws_reverse_host}
                        error={errors['onebot/ws_reverse_host']}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(v) => set({ ws_reverse_host: v })}
                    />
                    <NumberField
                        label="ws_reverse_port"
                        value={row.ws_reverse_port}
                        min={1}
                        max={65535}
                        error={errors['onebot/ws_reverse_port']}
                        disabled={lockPort}
                        hint={linked ? '已对接，改口会重写协议 Bot 侧连接' : undefined}
                        onValueChange={(v) => set({ ws_reverse_port: v ?? row.ws_reverse_port })}
                    />
                    <TextField
                        label="ws_reverse_token"
                        value={row.ws_reverse_token}
                        disabled={disabled}
                        className="font-mono sm:col-span-2"
                        onValueChange={(v) => set({ ws_reverse_token: v })}
                    />
                </div>
                <Switch
                    label="enable"
                    checked={row.enable}
                    disabled={lockPort}
                    onCheckedChange={(enable) => set({ enable })}
                />
                {!config.claimed && (
                    <p className="text-muted-foreground text-sm">
                        还没有认领的 OneBot v11。零条会在对接时加一条；有多条请在 AstrBot WebUI 指定。
                    </p>
                )}
            </FormSection>
            {config.other_platforms.length > 0 && (
                <FormSection title="其它平台">
                    <p className="text-muted-foreground text-sm">
                        {config.other_platforms.join('、')} · 在 AstrBot WebUI 里管
                    </p>
                </FormSection>
            )}
            {footer}
        </ConfigForm>
    );
};
