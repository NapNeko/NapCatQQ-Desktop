// 调试请求的关联 id：前端生成，后端用它把「取消」对上正在等待的那次调用。

/** 优先用浏览器自带的 randomUUID；非安全上下文（部分 http 预览）里它不存在，退回手拼的 v4 */
export function newRequestId(): string {
    const c = (globalThis as { crypto?: Crypto }).crypto;
    if (c && typeof c.randomUUID === 'function') return c.randomUUID();

    const bytes = new Uint8Array(16);
    if (c && typeof c.getRandomValues === 'function') {
        c.getRandomValues(bytes);
    } else {
        // 连 getRandomValues 都没有时只求不撞：这个 id 不做任何安全用途
        for (let i = 0; i < bytes.length; i += 1) bytes[i] = Math.floor(Math.random() * 256);
    }
    // RFC 4122 v4：第 7 字节高 4 位是版本，第 9 字节高 2 位是变体
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    const hex = Array.from(bytes, (b) => b.toString(16).padStart(2, '0')).join('');
    return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}
