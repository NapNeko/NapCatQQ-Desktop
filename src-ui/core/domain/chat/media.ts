// 媒体读取选项与转发投影的数据形状;带 AbortSignal 的读取流程本身仍由 service 层持有。
import type { Segment } from '../debug/segments';

export interface ForwardNode {
    senderId: string;
    name: string;
    time?: number;
    segments: Segment[];
}
export interface FavoriteEmoji {
    url: string;
    description: string;
}
export interface ImageSourceContext {
    messageId?: string | number;
    imageIndex?: number;
    preferProtocol?: boolean;
}
export interface ImageReadOptions {
    signal?: AbortSignal;
    onReadStart?: () => void;
    context?: ImageSourceContext;
}
