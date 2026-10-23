import { Button, NumberField, Switch } from '../../../shared/ui';
import { useMcpSettings } from '../../../hooks/settings/useMcpSettings';
import type { SettingsDraft } from '../settings-draft';
import { FieldRow, SettingsSection } from '../_shared';

export function McpSection({
    draft,
    patchDraft,
}: {
    draft: SettingsDraft;
    patchDraft: (patch: Partial<SettingsDraft>) => void;
}) {
    const { status, error, busy, feedback, copy, rotate } = useMcpSettings();
    const patch = (value: Partial<SettingsDraft['mcp']>) =>
        patchDraft({ mcp: { ...draft.mcp, ...value } });
    const listening = status?.listening === true;
    return (
        <SettingsSection
            title="MCP"
            description="供本机 AI 客户端读取运维状态和调用 OneBot。保存设置后生效。"
        >
            <FieldRow label="启用 MCP 服务" description="仅监听 127.0.0.1，所有请求均需访问令牌。">
                <Switch
                    aria-label="启用 MCP 服务"
                    checked={draft.mcp.enabled}
                    onCheckedChange={(enabled) => patch({ enabled })}
                />
            </FieldRow>
            <FieldRow label="监听端口" description="0 表示首次启动时自动分配空闲端口。">
                <NumberField
                    className="w-28"
                    min={0}
                    max={65535}
                    value={draft.mcp.port}
                    disabled={!draft.mcp.enabled}
                    aria-label="MCP 监听端口"
                    onValueChange={(port) => {
                        if (port !== null)
                            patch({ port: Math.max(0, Math.min(65535, Math.round(port))) });
                    }}
                />
            </FieldRow>
            <FieldRow
                label="只读模式"
                description="关闭后，客户端可发送消息或修改调试数据。确认令牌由客户端回传，不等于人工审批。"
            >
                <Switch
                    aria-label="MCP 只读模式"
                    checked={draft.mcp.readOnly}
                    disabled={!draft.mcp.enabled}
                    onCheckedChange={(readOnly) =>
                        patch({ readOnly, ...(readOnly ? { allowDangerous: false } : {}) })
                    }
                />
            </FieldRow>
            <FieldRow
                label="允许危险操作"
                description="包括撤回、踢人、清空历史及未经审核的动作；仍需回传一次性确认令牌。"
            >
                <Switch
                    aria-label="MCP 允许危险操作"
                    checked={draft.mcp.allowDangerous}
                    disabled={!draft.mcp.enabled || draft.mcp.readOnly}
                    onCheckedChange={(allowDangerous) => patch({ allowDangerous })}
                />
            </FieldRow>
            <FieldRow
                label="客户端连接"
                isLast
                description="复制的配置含访问令牌；重置会立即使旧配置失效。"
            >
                <div className="flex flex-col items-end gap-2">
                    <span className="text-[12px] text-text-secondary">
                        {listening
                            ? `http://127.0.0.1:${status.port}/mcp`
                            : status?.enabled
                              ? '服务未启动'
                              : '服务已关闭'}
                    </span>
                    <div className="flex gap-2">
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={!listening || busy}
                            onClick={copy}
                        >
                            复制连接配置
                        </Button>
                        <Button
                            size="sm"
                            variant="secondary"
                            disabled={!listening || busy}
                            onClick={rotate}
                        >
                            重置访问令牌
                        </Button>
                    </div>
                    {(feedback || status?.lastError || error) && (
                        <p role="status" className="max-w-md text-[12px] text-text-secondary">
                            {feedback ?? status?.lastError ?? error}
                        </p>
                    )}
                </div>
            </FieldRow>
        </SettingsSection>
    );
}
