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
                本机守护进程与远端 Native「密码接管」共用这里的覆盖值。留空：本机沿用
                session（首次自动生成）；远端勾选接管时每次启动生成新密码。填写自定义密码：本机下次启动
                与远端接管启动都会用这个值覆盖写入。未勾选接管时不会改远端已有密码。
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
                placeholder="留空 = 本机用 session；远端接管则每次生成"
                hint="非空时写入 app-config.json，本机与远端接管都优先用这个固定密码"
            />
        </FormSection>
    );
}