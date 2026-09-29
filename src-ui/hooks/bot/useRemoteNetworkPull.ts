// 远端 Bot 回读远端 onebot 网络配置。只读，不改 bot.json；读回来的内容由配置页填进表单，
// 用户确认后走正常保存。

import { useMutation } from '@tanstack/react-query';
import { botService } from '../../core/services/bot.service';

export function useRemoteNetworkPull() {
    const pull = useMutation({ mutationFn: botService.fetchBotRemoteNetwork });
    return {
        pullRemoteNetwork: pull.mutateAsync,
        pulling: pull.isPending,
    };
}
