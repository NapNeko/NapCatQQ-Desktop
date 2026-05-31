// 部署成功后的结果展示：WebUI / noVNC 地址 + 凭据。凭据只在这次展示一次
// （后端不持久化明文），提示用户记下来。
//
// 两种用法共享同一份内容渲染(DeployResultBody)：
//   - DeployResultBanner：外层框架行下方的横幅，带成功边框 + 可关闭。
//   - DeployDialog 完成态：直接内嵌 body，弹窗自己管标题和关闭。

import React from 'react';
import { CheckCircle2, X, ExternalLink } from 'lucide-react';
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
                        {result.name} 部署完成
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

// 结果内容：WebUI/noVNC 链接 + 凭据。不带外框/标题/关闭，供 banner 和
// 部署对话框完成态共用。
export const DeployResultBody: React.FC<{ result: DeployedContainer }> = ({ result }) => (
    <div className="flex flex-col gap-1.5 text-sm">
        <LinkRow label="WebUI" url={result.webuiUrl} />
        {result.novncUrl && <LinkRow label="noVNC" url={result.novncUrl} />}
        {result.webuiSecret && (
            <div className="flex items-center gap-2">
                <span className="w-14 shrink-0 text-text-tertiary">凭据</span>
                <code className="select-all rounded-xs bg-canvas/60 px-1.5 py-0.5 font-mono text-xs text-text">
                    {result.webuiSecret}
                </code>
                <span className="text-2xs text-text-tertiary">请记下，仅展示一次</span>
            </div>
        )}
    </div>
);

const LinkRow: React.FC<{ label: string; url: string }> = ({ label, url }) => (
    <div className="flex items-center gap-2">
        <span className="w-14 shrink-0 text-text-tertiary">{label}</span>
        <a
            href={url}
            target="_blank"
            rel="noreferrer"
            className="inline-flex items-center gap-1 text-brand hover:underline"
        >
            {url}
            <ExternalLink size={12} />
        </a>
    </div>
);
