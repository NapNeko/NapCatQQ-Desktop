// 收口 ChatTimeline / ChatSearch 的图片、合并转发、语音、视频、逐条转写读取。
// 这些读取由每条消息逐段按需触发并携带 per-call AbortSignal(服务内部还有限流排队),
// 不是"组件状态驱动的查询",套 useQuery 会丢取消语义,因此保持回调转发。
import { useMemo, useRef } from 'react';
import type { DebugTarget } from '../../core/ipc/generated/debug/DebugTarget';
import type { ForwardNode, ImageReadOptions } from '../../core/domain/chat/media';
import { chatMediaService } from '../../core/services/chat-media.service';

export interface ChatMediaReader {
    isImageSourceAlive: (data: Record<string, unknown>) => boolean;
    image: (
        data: Record<string, unknown>,
        refresh?: boolean,
        options?: ImageReadOptions,
    ) => Promise<string>;
    forward: (data: Record<string, unknown>) => Promise<ForwardNode[]>;
    record: (data: Record<string, unknown>) => Promise<string>;
    video: (data: Record<string, unknown>, refresh?: boolean) => Promise<string>;
    transcript: (messageId: string) => Promise<string>;
}

/** getTarget 在每次调用时求值，保持调用点直读 store.target 的即时语义。 */
export function useChatMedia(getTarget: () => DebugTarget): ChatMediaReader {
    const getter = useRef(getTarget);
    getter.current = getTarget;
    return useMemo(
        () => ({
            isImageSourceAlive: (data) =>
                chatMediaService.isImageSourceAlive(getter.current(), data),
            image: (data, refresh = false, options = {}) =>
                chatMediaService.image(getter.current(), data, refresh, options),
            forward: (data) => chatMediaService.forward(getter.current(), data),
            record: (data) => chatMediaService.record(getter.current(), data),
            video: (data, refresh = false) =>
                chatMediaService.video(getter.current(), data, refresh),
            transcript: (messageId) => chatMediaService.transcript(getter.current(), messageId),
        }),
        [],
    );
}
