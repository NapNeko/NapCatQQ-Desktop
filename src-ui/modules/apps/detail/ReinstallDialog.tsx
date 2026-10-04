// 「重装 / 换版本」对话框：给已安装的实例换一个版本重装。
//
// 为什么单独一个对话框而不是就地一个下拉：重装会走完整安装流程（重新拉依赖、
// 覆盖脚手架），是个有副作用的动作，值得一次明确确认。文案里把「当前版本 →
// 目标版本」写出来，避免用户点完不知道会发生什么。

import React from 'react';
import {
    Button,
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
    Spinner,
} from '../../../shared/ui';
import { VersionSelect } from '../VersionSelect';
import type { AppInstance } from '../../../core/ipc/types';

export const ReinstallDialog: React.FC<{
    open: boolean;
    instance: AppInstance;
    version: string | null;
    onVersionChange: (version: string | null) => void;
    latestVersion: string | null;
    busy: boolean;
    onClose: () => void;
    onConfirm: () => void | Promise<void>;
}> = ({ open, instance, version, onVersionChange, latestVersion, busy, onClose, onConfirm }) => {
    const current = instance.installed_version?.trim() || null;
    const target = version ?? latestVersion;
    const targetText = target ? target : '最新正式版';
    const same = !!current && !!target && current === target;

    return (
        <Dialog open={open} onOpenChange={(next) => !next && onClose()}>
            <DialogContent className="max-w-md">
                <DialogHeader>
                    <DialogTitle>重装 {instance.display_name}</DialogTitle>
                    <DialogDescription>
                        {busy
                            ? '实例正在忙，等当前操作结束后再试。'
                            : '会重新同步依赖并覆盖桌面端写过的脚手架文件；用户自己的插件与配置保持不动。'}
                    </DialogDescription>
                </DialogHeader>

                <div className="flex flex-col gap-3">
                    <div className="flex flex-col gap-1 rounded-sm border border-border-subtle bg-inset/40 px-3 py-2 text-xs">
                        <span className="text-text-secondary">
                            当前版本：
                            <span className="ml-1 font-mono text-text">
                                {current ? `v${current}` : '未知'}
                            </span>
                        </span>
                        <span className="text-text-secondary">
                            将重装：
                            <span className="ml-1 font-mono text-text">{targetText}</span>
                        </span>
                        {same && (
                            <span className="text-text-tertiary">
                                与当前版本相同——重装会重新同步依赖，但不会换代码
                            </span>
                        )}
                    </div>

                    {/* 不支持按版本安装的框架里，VersionSelect 自己返回 null */}
                    <VersionSelect
                        frameworkId={instance.framework_id}
                        value={version}
                        onChange={onVersionChange}
                        disabled={busy}
                        hint="选「最新正式版」跟随上游；也可以钉住某个历史版本回退"
                    />
                </div>

                <DialogFooter>
                    <Button variant="ghost" size="sm" onClick={onClose} disabled={busy}>
                        取消
                    </Button>
                    <Button variant="primary" size="sm" disabled={busy} onClick={() => void onConfirm()}>
                        {busy && <Spinner size="xs" className="text-white" />}
                        重装
                    </Button>
                </DialogFooter>
            </DialogContent>
        </Dialog>
    );
};
