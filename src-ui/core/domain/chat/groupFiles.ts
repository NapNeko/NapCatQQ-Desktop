// 群文件列表的排序、筛选、权限和文案；不碰 IPC，方便单测。
import type { GroupFile } from '../../ipc/generated/chat/GroupFile';
import type { GroupFolder } from '../../ipc/generated/chat/GroupFolder';
import type { ChatFileSource } from '../../ipc/generated/chat/ChatFileSource';
import type { DebugStreamProgress } from '../../ipc/generated/debug/DebugStreamProgress';

export type GroupFileSort = 'time' | 'name' | 'size';
export type GroupFileRow =
    { kind: 'folder'; folder: GroupFolder } | { kind: 'file'; file: GroupFile };
export const ROOT_FOLDER = '/';

const collator = new Intl.Collator('zh-CN', { numeric: true, sensitivity: 'base' });

/** 文件夹总在前面；按大小排时文件夹按文件数 */
export function groupFileRows(
    folders: GroupFolder[],
    files: GroupFile[],
    sort: GroupFileSort,
    query: string,
): GroupFileRow[] {
    const term = query.trim().toLocaleLowerCase();
    const hit = (name: string, by: string) =>
        !term || name.toLocaleLowerCase().includes(term) || by.toLocaleLowerCase().includes(term);
    const byFolder = (a: GroupFolder, b: GroupFolder) =>
        sort === 'name'
            ? collator.compare(a.name, b.name)
            : sort === 'size'
              ? b.fileCount - a.fileCount || collator.compare(a.name, b.name)
              : b.createTime - a.createTime || collator.compare(a.name, b.name);
    const byFile = (a: GroupFile, b: GroupFile) =>
        sort === 'name'
            ? collator.compare(a.name, b.name)
            : sort === 'size'
              ? b.size - a.size || collator.compare(a.name, b.name)
              : b.uploadTime - a.uploadTime || collator.compare(a.name, b.name);
    return [
        ...folders
            .filter((f) => hit(f.name, f.creatorName))
            .sort(byFolder)
            .map((folder) => ({ kind: 'folder' as const, folder })),
        ...files
            .filter((f) => hit(f.name, f.uploaderName))
            .sort(byFile)
            .map((file) => ({ kind: 'file' as const, file })),
    ];
}

/** 临时文件还剩多久；永久文件给空串 */
export function expiryLabel(deadTime: number, nowSeconds = Date.now() / 1000): string {
    if (!deadTime) return '';
    const left = deadTime - nowSeconds;
    if (left <= 0) return '已过期';
    const days = Math.ceil(left / 86400);
    return days <= 1 ? '今天过期' : `${days} 天后过期`;
}

export function fileDate(seconds: number, nowSeconds = Date.now() / 1000): string {
    if (!seconds) return '';
    const date = new Date(seconds * 1000);
    const now = new Date(nowSeconds * 1000);
    const pad = (n: number) => String(n).padStart(2, '0');
    if (date.getFullYear() !== now.getFullYear())
        return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
    return `${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

const isManager = (role: string) => role === 'owner' || role === 'admin';
/** 角色没读到时不拦，交给上游判 */
export const canManageFolders = (role: string | null) => role === null || isManager(role);
export const canChangeFile = (file: GroupFile, selfId: string, role: string | null) =>
    role === null || isManager(role) || file.uploader === selfId;

export type FileKind =
    'image' | 'video' | 'audio' | 'archive' | 'sheet' | 'slide' | 'doc' | 'code' | 'app' | 'other';
const KINDS: [FileKind, RegExp][] = [
    ['image', /\.(png|jpe?g|gif|webp|bmp|svg|heic|ico)$/i],
    ['video', /\.(mp4|mkv|mov|avi|wmv|flv|webm)$/i],
    ['audio', /\.(mp3|wav|flac|m4a|aac|ogg|amr|silk)$/i],
    ['archive', /\.(zip|rar|7z|tar|gz|tgz|xz|bz2)$/i],
    ['sheet', /\.(xlsx?|csv|numbers|et)$/i],
    ['slide', /\.(pptx?|key|dps)$/i],
    ['doc', /\.(docx?|pdf|txt|md|rtf|wps|epub)$/i],
    ['code', /\.(json|ya?ml|toml|ini|js|ts|py|rs|java|c|cpp|go|sh|bat|ps1|xml|html?|css|log)$/i],
    ['app', /\.(exe|msi|apk|dmg|deb|rpm|appimage|jar)$/i],
];
export const fileKind = (name: string): FileKind =>
    KINDS.find(([, pattern]) => pattern.test(name))?.[0] ?? 'other';

/** 百分比；总量未知时为 null，界面只显示已传量 */
export function transferPercent(
    progress: Pick<DebugStreamProgress, 'done_bytes' | 'total_bytes'> | undefined,
): number | null {
    if (!progress?.total_bytes) return null;
    return Math.min(100, Math.round((progress.done_bytes / progress.total_bytes) * 100));
}

const str = (value: unknown) =>
    typeof value === 'string'
        ? value
        : typeof value === 'number' && Number.isFinite(value)
          ? String(value)
          : '';
/** 时间线文件段 → 下载来源。没有 file_id 的段（自己刚发、还没回显）下不了 */
export function fileSegmentSource(
    data: Record<string, unknown>,
    session: { type: 'group' | 'private'; id: string },
): ChatFileSource | null {
    const fileId = str(data.file_id);
    if (!fileId) return null;
    if (session.type === 'group') {
        const busid = Number(data.busid);
        return {
            kind: 'group',
            groupId: session.id,
            fileId,
            busid: Number.isFinite(busid) && busid > 0 ? busid : null,
        };
    }
    return { kind: 'private', userId: session.id, fileId, fileHash: str(data.file_hash) || null };
}

export const fileSegmentName = (data: Record<string, unknown>) =>
    str(data.name) || str(data.file_name) || str(data.file) || '文件';
