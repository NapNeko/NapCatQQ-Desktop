// Bot 列表页的加载 / 错误 / 空态三种占位视图。从页面主组件搬出，
// 页面只按 react-query 状态挑一个渲染，不承担这三块的排版细节。

import { Bot } from 'lucide-react';
import { Button, Spinner } from '../../../../shared/ui';
import { MotionIcon } from '../../../../shared/ui/motion';
import { PagePlaceholder } from '../../../../shared/ui/PagePlaceholder';

export function LoadingState() {
    return (
        <div className="flex flex-1 flex-col items-center justify-center gap-3 py-20 text-text-tertiary">
            <Spinner size="lg" />
            <p className="text-sm">正在加载 Bot 实例…</p>
        </div>
    );
}

export function ErrorState({ onRetry }: { onRetry: () => void }) {
    return (
        <PagePlaceholder className="gap-3">
            <p className="text-sm text-text-secondary">Bot 列表加载失败，详情见顶部提示条。</p>
            <Button size="sm" variant="primary" onClick={onRetry}>
                重试
            </Button>
        </PagePlaceholder>
    );
}

export function EmptyState({ onCreate, onImport }: { onCreate: () => void; onImport: () => void }) {
    return (
        <PagePlaceholder className="gap-4">
            <MotionIcon
                icon={Bot}
                motion="bob"
                playEnter
                enterKey="empty-bot"
                size={32}
                strokeWidth={1.6}
                className="text-text-tertiary"
            />
            <div>
                <p className="font-display text-md font-semibold text-text">还没有 Bot 实例</p>
            </div>
            <div className="flex flex-wrap items-center justify-center gap-2">
                <Button size="sm" variant="secondary" onClick={onImport}>
                    导入已有 Bot
                </Button>
                <Button
                    size="sm"
                    variant="primary"
                    onClick={onCreate}
                    data-tour-id="bot-create-empty"
                >
                    创建第一个实例
                </Button>
            </div>
        </PagePlaceholder>
    );
}
