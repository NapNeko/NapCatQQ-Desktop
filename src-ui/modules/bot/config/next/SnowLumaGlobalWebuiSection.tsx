// SnowLuma 全局 WebUI：与 Bot 字段一并由配置页保存。

import { NumberField, TextField, FormSection } from '../../../../shared/ui';
import type { SnowLumaAppConfig } from '../../../../core/ipc/generated/domain/SnowLumaAppConfig';

export interface SnowLumaGlobalWebuiSectionProps {
    value: SnowLumaAppConfig;
    onChange: (next: SnowLumaAppConfig) => void;
    loadError?: string | null;
    loading?: boolean;
}

export function SnowLumaGlobalWebuiSection({
    value,
    onChange,
    loadError,
    loading,
}: SnowLumaGlobalWebuiSectionProps) {
    if (loadError) {
        return (
            <FormSection title="SnowLuma WebUI（全局）">
                <p className="text-2xs text-text-secondary">无法加载全局 WebUI 配置，详情见日志</p>
            </FormSection>
        );
    }

    if (loading) {
        return (
            <FormSection title="SnowLuma WebUI（全局）">
                <p className="text-2xs text-text-tertiary">加载中…</p>
            </FormSection>
        );
    }

    return (
        <FormSection
            title="SnowLuma WebUI（全局）"
            description="本机与远端接管共用；留空则本机用 session，远端接管每次生成"
        >
            <NumberField
                label="WebUI 监听端口"
                value={value.snowlumaWebuiPort}
                onValueChange={(v) =>
                    onChange({ ...value, snowlumaWebuiPort: v ?? 5099 })
                }
                min={1}
                max={65535}
                hint="被占用时改用附近端口，需重启守护进程"
            />
            <TextField
                label="WebUI 登录密码（可选覆盖）"
                value={value.snowlumaWebuiPasswordOverride}
                onValueChange={(v) =>
                    onChange({ ...value, snowlumaWebuiPasswordOverride: v })
                }
            />
        </FormSection>
    );
}