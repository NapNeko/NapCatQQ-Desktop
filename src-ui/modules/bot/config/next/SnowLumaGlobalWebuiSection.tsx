// SnowLuma 全局 WebUI（daemon 单例）：受控表单项，由配置页右上角统一保存。

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
                <p className="text-2xs text-danger">无法加载全局 WebUI 配置：{loadError}</p>
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
            description="本机 SnowLuma 守护进程共用一个 WebUI；与上方 Bot 字段一并由右上角「保存」写入"
        >
            <p className="rounded-sm border border-border-subtle bg-canvas/60 px-3 py-2.5 text-2xs leading-relaxed text-text-secondary">
                仅作用于本机 SnowLuma 守护进程。远端 SSH「直接运行」的 WebUI 密码由远端主机上的
                secret 管理，在 Bot 列表打开 WebUI 时复制到剪贴板。本机密码留空时，每次启动守护进程会
                自动生成并写入 session；若填写自定义密码，则下次启动时用你设置的值覆盖。
            </p>
            <NumberField
                label="WebUI 监听端口"
                value={value.snowlumaWebuiPort}
                onValueChange={(v) =>
                    onChange({ ...value, snowlumaWebuiPort: v ?? 5099 })
                }
                min={1}
                max={65535}
                hint="默认 5099；启动时若被占用会自动改用附近空闲端口（写入 runtime.json），需重启守护进程后生效"
            />
            <TextField
                label="WebUI 登录密码（可选覆盖）"
                value={value.snowlumaWebuiPasswordOverride}
                onValueChange={(v) =>
                    onChange({ ...value, snowlumaWebuiPasswordOverride: v })
                }
                placeholder="留空 = 每次启动自动生成"
                hint="非空时写入 app-config.json，优先于自动生成的 session 密码"
            />
        </FormSection>
    );
}