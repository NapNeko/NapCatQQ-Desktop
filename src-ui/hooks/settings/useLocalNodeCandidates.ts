// 设置页「运行环境」里可选的本机 Node：每次进这页都重新探测，只列能用的。

import { useQuery } from '@tanstack/react-query';
import { componentService } from '../../core/services/component.service';
import type { NodeEnvironmentCandidate } from '../../core/ipc/types';

const NO_CANDIDATES: NodeEnvironmentCandidate[] = [];

export function useLocalNodeCandidates() {
    const query = useQuery<NodeEnvironmentCandidate[], Error>({
        queryKey: ['localNodeCandidates'],
        queryFn: async () => {
            try {
                const list = await componentService.probeLocalNodeCandidates();
                return list.filter((c) => c.isValid);
            } catch (err) {
                // 探测失败按一个都没有处理，下拉里还剩「自动选择」
                console.error('[RuntimeTab] probe node candidates error:', err);
                return [];
            }
        },
        staleTime: 0,
        gcTime: 0,
    });

    return {
        candidates: query.data ?? NO_CANDIDATES,
        probing: query.isFetching,
        /** 至少探完一轮，才能判断已选的 Node 是否失效 */
        probeComplete: query.isFetched && !query.isFetching,
        reprobe: () => void query.refetch(),
    };
}
