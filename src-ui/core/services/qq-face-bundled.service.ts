// 本地单帧首图随应用发布，动态资源仍按消息 wire 标记读取。
import assets from '../domain/chat/qqFaceAssets.json';
const bundled = new Set(assets.ids);
export function bundledQQFaceSource(id: string): string {
    return bundled.has(id) ? import.meta.env.BASE_URL + 'qq-faces/' + id + '.png' : '';
}
