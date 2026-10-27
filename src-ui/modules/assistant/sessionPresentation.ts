// 会话状态在标题与列表中使用相同的语义色。

import type { AssistantSessionState } from '../../core/domain/assistant/types';

type Tone = 'neutral' | 'brand' | 'success' | 'warning' | 'danger';

export const sessionStateTone: Record<AssistantSessionState, Tone> = {
    starting: 'brand',
    ready: 'success',
    running: 'brand',
    waitingForInput: 'warning',
    disconnected: 'neutral',
    closed: 'neutral',
    failed: 'danger',
};
