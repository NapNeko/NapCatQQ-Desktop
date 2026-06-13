// 路由级错误边界：子树抛错时显示可恢复提示，避免整页白屏。

import { Component, type ErrorInfo, type ReactNode } from 'react';
import { Button } from './Button';

interface Props {
    children: ReactNode;
    title?: string;
}

interface State {
    error: Error | null;
}

export class RouteErrorBoundary extends Component<Props, State> {
    state: State = { error: null };

    static getDerivedStateFromError(error: Error): State {
        return { error };
    }

    componentDidCatch(error: Error, info: ErrorInfo): void {
        // eslint-disable-next-line no-console
        console.error('[RouteErrorBoundary]', error, info.componentStack);
    }

    render(): ReactNode {
        if (this.state.error) {
            return (
                <div className="flex flex-1 flex-col items-center justify-center gap-3 px-6 py-16 text-center">
                    <p className="font-display text-md font-semibold text-text">
                        {this.props.title ?? '页面渲染失败'}
                    </p>
                    <p className="max-w-md text-xs text-text-secondary break-words">
                        {this.state.error.message}
                    </p>
                    <Button
                        size="sm"
                        variant="primary"
                        onClick={() => this.setState({ error: null })}
                    >
                        重试
                    </Button>
                </div>
            );
        }
        return this.props.children;
    }
}