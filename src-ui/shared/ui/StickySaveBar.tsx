// 粘性保存条。配置页底部固定，永远在视线里。
//
// 状态：
//   - dirty=false: "✓ 已是最新状态" + 取消/保存按钮 disabled
//   - dirty=true:  "● 有未保存改动" + 按钮可点
//   - saving=true: 保存按钮转圈 + disabled，文案 "保存中…"
//
// 不弹 Toast，文案就在 bar 里就近显示，符合 "系统状态可见" 原则。

import { Save, AlertCircle, Check } from 'lucide-react';
import { type ReactNode } from 'react';
import { cn } from '../utils/cn';
import { Button } from './Button';
import { Spinner } from './Spinner';

export interface StickySaveBarProps {
    dirty: boolean;
    saving: boolean;
    onSave: () => void;
    onCancel?: () => void;
    /** 自定义状态文案（覆盖默认 "已是最新状态/有未保存改动"）。 */
    statusText?: ReactNode;
    saveLabel?: ReactNode;
    cancelLabel?: ReactNode;
    /** 左侧附加内容，比如校验错误的简短提示。 */
    extra?: ReactNode;
    className?: string;
}

export function StickySaveBar({
    dirty,
    saving,
    onSave,
    onCancel,
    statusText,
    saveLabel = '保存配置',
    cancelLabel = '撤销改动',
    extra,
    className,
}: StickySaveBarProps) {
    return (
        <div
            className={cn(
                'sticky bottom-0 z-10 mt-4 flex items-center justify-between gap-3',
                'border-t border-border-subtle bg-canvas/95 px-6 py-3 backdrop-blur-sm',
                className,
            )}
        >
            <div className="flex min-w-0 flex-1 items-center gap-2 text-xs">
                {dirty ? (
                    <span className="inline-flex items-center gap-1.5 text-warning">
                        <AlertCircle size={13} strokeWidth={2.4} />
                        <span>{statusText ?? '有未保存改动'}</span>
                    </span>
                ) : (
                    <span className="inline-flex items-center gap-1.5 text-text-tertiary">
                        <Check size={13} strokeWidth={2.4} />
                        <span>{statusText ?? '已是最新状态'}</span>
                    </span>
                )}
                {extra && <span className="ml-2 text-text-tertiary">{extra}</span>}
            </div>
            <div className="flex items-center gap-2">
                {onCancel && (
                    <Button
                        variant="ghost"
                        size="sm"
                        onClick={onCancel}
                        disabled={!dirty || saving}
                    >
                        {cancelLabel}
                    </Button>
                )}
                <Button
                    variant="primary"
                    size="sm"
                    onClick={onSave}
                    disabled={!dirty || saving}
                >
                    {saving ? (
                        <>
                            <Spinner size="xs" />
                            <span>保存中…</span>
                        </>
                    ) : (
                        <>
                            <Save size={13} strokeWidth={2.2} />
                            <span>{saveLabel}</span>
                        </>
                    )}
                </Button>
            </div>
        </div>
    );
}
