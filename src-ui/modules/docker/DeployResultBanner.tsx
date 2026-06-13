// 拉镜像成功后的结果展示（仅镜像名；无 WebUI 凭据）。

import React from 'react';
import { CheckCircle2, X } from 'lucide-react';
import type { DeployedContainer } from '../../core/ipc/types';

interface DeployResultBannerProps {
    result: DeployedContainer;
    onDismiss: () => void;
}

export const DeployResultBanner: React.FC<DeployResultBannerProps> = ({
    result,
    onDismiss,
}) => {
    return (
        <div className="flex flex-col gap-3 rounded-md border border-success/30 bg-success-soft px-4 py-3">
            <div className="flex items-start justify-between gap-2">
                <div className="flex items-center gap-2">
                    <CheckCircle2 size={16} className="text-success" />
                    <span className="text-sm font-medium text-text">
                        {result.flavor === 'napcat' ? 'NapCat' : 'SnowLuma'} 镜像已就绪
                    </span>
                </div>
                <button
                    onClick={onDismiss}
                    aria-label="关闭"
                    className="rounded-xs p-1 text-text-tertiary hover:bg-inset hover:text-text"
                >
                    <X size={14} />
                </button>
            </div>

            <DeployResultBody result={result} />
        </div>
    );
};

export const DeployResultBody: React.FC<{ result: DeployedContainer }> = ({ result }) => (
    <div className="flex flex-col gap-1.5 text-sm">
        <div className="flex items-center gap-2">
            <span className="w-14 shrink-0 text-text-tertiary">镜像</span>
            <code className="select-all rounded-xs bg-canvas/60 px-1.5 py-0.5 font-mono text-xs text-text">
                {result.image}
            </code>
        </div>
        <p className="text-2xs text-text-tertiary">
            Bot 容器在 Bot 页启动时自动创建（ncbot-&lt;QQ号&gt;）。
        </p>
    </div>
);