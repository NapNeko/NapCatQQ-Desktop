import { useId, useState } from 'react';
import { Copy } from 'lucide-react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    NumberField,
    Select,
    Switch,
} from '../../../shared/ui';
import { ExpandChevron, ExpandPresence } from '../../../shared/ui/motion';
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
    const [advanced, setAdvanced] = useState(false);
    const [confirmRotate, setConfirmRotate] = useState(false);
    const advancedId = useId();
    const patch = (value: Partial<SettingsDraft['mcp']>) =>
        patchDraft({ mcp: { ...draft.mcp, ...value } });
    const listening = status?.listening === true;
    const serviceError = status?.lastError ?? error;
    const permission = draft.mcp.readOnly
        ? 'readOnly'
        : draft.mcp.allowDangerous
          ? 'dangerous'
          : 'write';

    return (
        <>
            <SettingsSection
                title="MCP"
                description="让本机 AI 客户端查看 Bot 状态、日志和 OneBot 能力。保存设置后生效。"
            >
                <FieldRow
                    label="启用 MCP 服务"
                    description="仅供本机连接，默认只读，使用访问令牌认证。"
                >
                    <Switch
                        aria-label="启用 MCP 服务"
                        checked={draft.mcp.enabled}
                        onCheckedChange={(enabled) => patch({ enabled })}
                    />
                </FieldRow>
                <ExpandPresence visible={draft.mcp.enabled}>
                    <div className="divide-y divide-border-subtle/70 pt-4">
                        <FieldRow
                            label="客户端连接"
                            description={
                                listening
                                    ? `127.0.0.1:${status.port}/mcp · 配置含访问令牌，请妥善保管。`
                                    : status?.enabled
                                      ? '服务未启动，请检查下方状态。'
                                      : '保存设置后即可复制连接配置。'
                            }
                        >
                            <Button size="sm" disabled={!listening || busy} onClick={copy}>
                                <Copy size={13} />
                                复制连接配置
                            </Button>
                        </FieldRow>
                        <div className="pt-3">
                            <button
                                type="button"
                                className="inline-flex items-center gap-2 rounded-xs py-1 text-xs text-text-tertiary transition-colors hover:text-text focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand"
                                aria-expanded={advanced}
                                aria-controls={advancedId}
                                onClick={() => setAdvanced((value) => !value)}
                            >
                                <ExpandChevron open={advanced} />
                                高级设置
                                <span className="text-text-disabled">
                                    {draft.mcp.readOnly
                                        ? '只读访问'
                                        : draft.mcp.allowDangerous
                                          ? '允许危险操作'
                                          : '允许写入'}
                                </span>
                            </button>
                            <ExpandPresence visible={advanced}>
                                <div
                                    id={advancedId}
                                    className="mt-3 divide-y divide-border-subtle/70"
                                >
                                    <FieldRow
                                        label="访问权限"
                                        description={
                                            permission === 'readOnly'
                                                ? '只查看信息，不发送消息或修改数据。'
                                                : permission === 'dangerous'
                                                  ? '允许撤回、踢人、清空历史等已审核的危险动作。确认令牌由客户端回传，不等于人工审批。'
                                                  : '允许发送消息和修改调试数据。确认令牌由客户端回传，不等于人工审批。'
                                        }
                                    >
                                        <Select
                                            aria-label="MCP 访问权限"
                                            className="w-44"
                                            value={permission}
                                            items={[
                                                { value: 'readOnly', label: '只读' },
                                                { value: 'write', label: '允许写入' },
                                                { value: 'dangerous', label: '含危险操作' },
                                            ]}
                                            onValueChange={(value) =>
                                                patch({
                                                    readOnly: value === 'readOnly',
                                                    allowDangerous: value === 'dangerous',
                                                })
                                            }
                                        />
                                    </FieldRow>
                                    <FieldRow
                                        label="监听端口"
                                        description="0 表示首次启动时自动分配空闲端口。"
                                    >
                                        <NumberField
                                            className="w-28"
                                            min={0}
                                            max={65535}
                                            value={draft.mcp.port}
                                            aria-label="MCP 监听端口"
                                            onValueChange={(port) => {
                                                if (port !== null)
                                                    patch({
                                                        port: Math.max(
                                                            0,
                                                            Math.min(65535, Math.round(port)),
                                                        ),
                                                    });
                                            }}
                                        />
                                    </FieldRow>
                                    <FieldRow
                                        label="访问令牌"
                                        description="重置立即生效，已连接的客户端需要更新配置。"
                                    >
                                        <Button
                                            size="sm"
                                            variant="ghost"
                                            disabled={!listening || busy}
                                            onClick={() => setConfirmRotate(true)}
                                        >
                                            重置令牌…
                                        </Button>
                                    </FieldRow>
                                </div>
                            </ExpandPresence>
                        </div>
                    </div>
                </ExpandPresence>
                {(serviceError || feedback) && (
                    <p
                        role={serviceError ? 'alert' : 'status'}
                        className={`pt-3 text-xs leading-relaxed ${serviceError ? 'text-danger' : 'text-text-secondary'}`}
                    >
                        {serviceError ?? feedback}
                    </p>
                )}
            </SettingsSection>
            <Dialog open={confirmRotate} onOpenChange={setConfirmRotate}>
                <DialogContent size="sm">
                    <DialogHeader>
                        <DialogTitle>重置 MCP 访问令牌</DialogTitle>
                        <DialogDescription>
                            旧连接配置将立即失效。重置后，请重新复制配置并更新 AI 客户端。
                        </DialogDescription>
                    </DialogHeader>
                    <DialogFooter>
                        <Button onClick={() => setConfirmRotate(false)}>取消</Button>
                        <Button
                            variant="primary"
                            disabled={!listening || busy}
                            onClick={() => {
                                rotate();
                                setConfirmRotate(false);
                            }}
                        >
                            重置访问令牌
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </>
    );
}
