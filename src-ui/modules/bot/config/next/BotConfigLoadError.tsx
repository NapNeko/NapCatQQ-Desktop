// 配置页的加载态/读失败态：编辑态拉配置期间的两种整页占位。

import { Button, Card, Spinner } from '../../../../shared/ui';

export function ConfigLoadingView() {
    return (
        <div className="flex h-full items-center justify-center">
            <Card className="flex flex-col items-center gap-3 px-10 py-8" variant="default">
                <Spinner size="lg" tone="brand" />
                <p className="text-sm text-text-secondary">正在读取配置文件…</p>
            </Card>
        </div>
    );
}

export function ConfigLoadErrorView({ onBack }: { onBack: () => void }) {
    return (
        <div className="flex h-full items-center justify-center">
            <Card className="flex max-w-md flex-col gap-3 px-6 py-5" variant="outlined">
                <h3 className="font-display text-md font-semibold text-text">读取配置失败</h3>
                <p className="text-sm text-text-secondary">详情见日志</p>
                <div>
                    <Button variant="secondary" size="sm" onClick={onBack}>
                        返回列表
                    </Button>
                </div>
            </Card>
        </div>
    );
}
