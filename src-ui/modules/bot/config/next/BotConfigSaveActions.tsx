// 粘性工具条上的保存/撤销区与连接数徽章。从配置页壳外提，纯展示。

import { AlertCircle, Check, Save } from 'lucide-react';
import { Button, Spinner } from '../../../../shared/ui';
import { ActionMotionIcon, infoToneMotion } from '../../../../shared/ui/motion';
import type { BotConfig } from '../../../../core/ipc/generated/domain/BotConfig';
import { countConnections } from './botConfigSync';

export function ConnectionCountBadge({ config }: { config: BotConfig }) {
    const count = countConnections(config);
    if (count === 0) return null;
    return (
        <span className="ml-1.5 inline-flex h-4 min-w-[16px] items-center justify-center rounded-pill bg-info-soft px-1 text-2xs font-medium text-info">
            {count}
        </span>
    );
}

interface SaveActionsProps {
    dirty: boolean;
    saving: boolean;
    onSave: () => void;
    onCancel: () => void;
    tourDemoMode?: boolean;
}

export function SaveActions({
    dirty,
    saving,
    onSave,
    onCancel,
    tourDemoMode = false,
}: SaveActionsProps) {
    return (
        <div className="flex shrink-0 items-center gap-3 pr-1" data-tour-id="bot-save-actions">
            <span className="hidden text-xs sm:inline-flex sm:items-center sm:gap-1.5">
                {tourDemoMode ? (
                    <span className="text-brand">演示 · 不会写入</span>
                ) : dirty ? (
                    <>
                        <ActionMotionIcon
                            icon={AlertCircle}
                            size={12}
                            strokeWidth={2.4}
                            motion={infoToneMotion('info')}
                            className="text-info"
                        />
                        <span className="text-info">未保存</span>
                    </>
                ) : (
                    <>
                        <ActionMotionIcon
                            icon={Check}
                            size={12}
                            strokeWidth={2.4}
                            className="text-text-tertiary"
                        />
                        <span className="text-text-tertiary">已是最新</span>
                    </>
                )}
            </span>
            <div className="flex items-center gap-1.5">
                <Button
                    variant="ghost"
                    size="sm"
                    onClick={onCancel}
                    disabled={!dirty || saving || tourDemoMode}
                >
                    撤销
                </Button>
                <Button
                    variant="primary"
                    size="sm"
                    onClick={onSave}
                    disabled={(!dirty && !tourDemoMode) || saving}
                >
                    {saving ? (
                        <>
                            <Spinner size="xs" />
                            <span>保存中</span>
                        </>
                    ) : (
                        <>
                            <ActionMotionIcon icon={Save} size={13} strokeWidth={2.2} />
                            <span>{tourDemoMode ? '保存（演示）' : '保存'}</span>
                        </>
                    )}
                </Button>
            </div>
        </div>
    );
}
