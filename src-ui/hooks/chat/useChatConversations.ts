// 登录与运行态事件及时刷新账号清单，轮询补偿丢失的事件。
import { useEffect } from 'react';
import { useQuery, useQueryClient } from '@tanstack/react-query';
import { chatService } from '../../core/services/chat.service';
import { subscribeDomainEvents } from '../../core/services/domain-event-hub';

const targetsKey = ['chat', 'targets'] as const;

export function useChatTargets() {
    const client = useQueryClient();
    useEffect(
        () =>
            subscribeDomainEvents((event) => {
                switch (event.kind) {
                    case 'bot_state_changed':
                    case 'napcat_login_online':
                    case 'napcat_login_invalidated':
                    case 'napcat_login_probe_unavailable':
                    case 'snowluma_login_state_changed':
                    case 'snowluma_login_probe_unavailable':
                    case 'snowluma_uin_detected':
                        void client.invalidateQueries({ queryKey: targetsKey });
                        break;
                }
            }),
        [client],
    );
    return useQuery({
        queryKey: targetsKey,
        queryFn: chatService.targets,
        refetchInterval: 15_000,
    });
}
