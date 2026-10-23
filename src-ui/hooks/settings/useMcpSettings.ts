import { useState } from 'react';
import { useQuery } from '@tanstack/react-query';
import { mcpService } from '../../core/services/mcp.service';

export function useMcpSettings() {
    const query = useQuery({
        queryKey: ['mcp-status'],
        queryFn: mcpService.status,
        refetchInterval: 5000,
    });
    const [busy, setBusy] = useState(false);
    const [feedback, setFeedback] = useState<string | null>(null);
    const run = async (action: () => Promise<void>, success: string) => {
        setBusy(true);
        setFeedback(null);
        try {
            await action();
            setFeedback(success);
        } catch (error) {
            setFeedback(error instanceof Error ? error.message : String(error));
        } finally {
            setBusy(false);
            void query.refetch();
        }
    };
    return {
        status: query.data,
        error: query.error?.message,
        busy,
        feedback,
        copy: () =>
            void run(mcpService.copyConnectionConfig, '连接配置已复制，请妥善保存其中的访问令牌'),
        rotate: () =>
            void run(mcpService.rotateToken, '访问令牌已重置，请重新复制配置并更新客户端'),
    };
}
