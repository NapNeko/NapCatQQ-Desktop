// 配置页顶栏：返回 / 标题副标题 / 删除入口。从配置页壳外提，纯展示。

import { ArrowLeft, Trash2 } from 'lucide-react';
import { Button } from '../../../../shared/ui';
import { ActionMotionIcon } from '../../../../shared/ui/motion';

interface BotConfigHeaderProps {
    isEditMode: boolean;
    tourDemoMode: boolean;
    botId: string | null;
    backendType: string;
    runtimeTarget: string;
    onBack: () => void;
    onRequestDelete: () => void;
}

export function BotConfigHeader({
    isEditMode,
    tourDemoMode,
    botId,
    backendType,
    runtimeTarget,
    onBack,
    onRequestDelete,
}: BotConfigHeaderProps) {
    return (
        <header
            className="flex items-start justify-between gap-3 border-b border-border-subtle py-3"
            data-tour-id="bot-config-header"
        >
            <div className="flex items-start gap-3">
                <Button
                    variant="ghost"
                    size="icon"
                    onClick={onBack}
                    aria-label="返回列表"
                    disabled={tourDemoMode}
                >
                    <ActionMotionIcon icon={ArrowLeft} size={16} />
                </Button>
                <div className="flex flex-col gap-0.5">
                    <h1 className="font-display text-md font-semibold text-text">
                        {tourDemoMode
                            ? '新建 Bot（演示）'
                            : isEditMode
                              ? '编辑 Bot 配置'
                              : '新建 Bot'}
                    </h1>
                    <p className="text-xs text-text-tertiary">
                        {tourDemoMode
                            ? '已预填演示数据，点保存不会写入配置'
                            : isEditMode
                              ? `QQ ${botId} · ${backendType} · ${runtimeTarget}`
                              : '至少添加一个连接才能与外部通信'}
                    </p>
                </div>
            </div>
            {isEditMode && !tourDemoMode && (
                <Button
                    variant="ghost"
                    size="sm"
                    className="text-danger hover:text-danger"
                    onClick={onRequestDelete}
                >
                    <ActionMotionIcon icon={Trash2} size={13} strokeWidth={2.2} />
                    <span>删除实例</span>
                </Button>
            )}
        </header>
    );
}
