// 表单改动的保存条：有改动或有错才出现，贴在内容区底部。没改东西时页面上不摆一个灰掉的保存按钮。

import { AlertCircle, Save } from 'lucide-react';
import { Button, Spinner } from '../../../shared/ui';
import { ActionMotionIcon, ExpandPresence, infoToneMotion } from '../../../shared/ui/motion';

export const SaveBar: React.FC<{
    dirty: boolean;
    saving: boolean;
    issueCount: number;
    onSave: () => void;
    onCancel: () => void;
    /** 错在别的页时给；保存按钮被错挡住，不给跳转就只能一页页去找 */
    onLocate?: () => void;
}> = ({ dirty, saving, issueCount, onSave, onCancel, onLocate }) => (
    <ExpandPresence visible={dirty || saving || issueCount > 0}>
        <div className="px-3 pb-3 pt-1">
            <div
                role="status"
                className="flex items-center justify-between gap-3 rounded-md border border-border-subtle bg-elevated px-4 py-2 shadow-popover"
            >
                <span className="flex min-w-0 items-center gap-1.5 text-[13px]">
                    {issueCount > 0 ? (
                        <>
                            <ActionMotionIcon
                                icon={AlertCircle}
                                size={14}
                                strokeWidth={2.2}
                                motion={infoToneMotion('danger')}
                                className="shrink-0 text-danger"
                            />
                            <span className="truncate text-danger">{issueCount} 处填写有误，改好才能保存</span>
                            {onLocate && (
                                <button
                                    type="button"
                                    onClick={onLocate}
                                    className="ml-1 shrink-0 rounded-xs text-brand underline-offset-2 hover:underline focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-brand/40"
                                >
                                    去看看
                                </button>
                            )}
                        </>
                    ) : saving ? (
                        <span className="text-text-secondary">正在保存…</span>
                    ) : (
                        <>
                            <ActionMotionIcon
                                icon={AlertCircle}
                                size={14}
                                strokeWidth={2.2}
                                motion={infoToneMotion('info')}
                                className="shrink-0 text-info"
                            />
                            <span className="truncate text-text">有改动还没保存</span>
                        </>
                    )}
                </span>
                <div className="flex shrink-0 items-center gap-1.5">
                    <Button variant="ghost" size="sm" onClick={onCancel} disabled={!dirty || saving}>
                        撤销
                    </Button>
                    <Button variant="primary" size="sm" onClick={onSave} disabled={!dirty || saving || issueCount > 0}>
                        {saving ? (
                            <>
                                <Spinner size="xs" />
                                <span>保存中</span>
                            </>
                        ) : (
                            <>
                                <ActionMotionIcon icon={Save} size={13} strokeWidth={2.2} />
                                <span>保存</span>
                            </>
                        )}
                    </Button>
                </div>
            </div>
        </div>
    </ExpandPresence>
);
