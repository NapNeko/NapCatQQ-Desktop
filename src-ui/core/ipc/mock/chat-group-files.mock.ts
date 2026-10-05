// 浏览器预览用的群文件：内存里一棵一层文件夹的树，形状和 runtime 归一后的一样。
import type { GroupFile } from '../generated/chat/GroupFile';
import type { GroupFolder } from '../generated/chat/GroupFolder';
import type { GroupFileListing } from '../generated/chat/GroupFileListing';
import type { GroupFileSpace } from '../generated/chat/GroupFileSpace';
import type { GroupFileAction } from '../generated/chat/GroupFileAction';
import type { DebugStreamProgress } from '../generated/debug/DebugStreamProgress';

interface Tree {
    folders: GroupFolder[];
    files: Map<string, GroupFile[]>;
}
const now = Math.floor(Date.now() / 1000);
const trees = new Map<string, Tree>();
const cancelled = new Set<string>();
let serial = 0;

const file = (
    name: string,
    size: number,
    ageDays: number,
    uploader: [string, string],
    temp = false,
): GroupFile => ({
    fileId: `mock-file-${++serial}`,
    name,
    busid: 102,
    size,
    uploadTime: now - ageDays * 86400,
    deadTime: temp ? now + (7 - ageDays) * 86400 : 0,
    modifyTime: now - ageDays * 86400,
    downloadTimes: (serial * 7) % 40,
    uploader: uploader[0],
    uploaderName: uploader[1],
});
const lin: [string, string] = ['10021', '小林'];
const cheng: [string, string] = ['10022', '阿澄'];

function tree(groupId: string): Tree {
    let value = trees.get(groupId);
    if (value) return value;
    value =
        groupId === '20001'
            ? {
                  folders: [
                      {
                          folderId: '/mock-docs',
                          name: '文档',
                          createTime: now - 86400 * 40,
                          creator: '10022',
                          creatorName: '阿澄',
                          fileCount: 3,
                      },
                      {
                          folderId: '/mock-release',
                          name: '版本发布',
                          createTime: now - 86400 * 12,
                          creator: '10021',
                          creatorName: '小林',
                          fileCount: 2,
                      },
                  ],
                  files: new Map([
                      [
                          '/',
                          [
                              file('NapCat 部署说明.pdf', 2_480_312, 3, cheng),
                              file('截图合集.zip', 48_201_337, 1, lin, true),
                              file('config.example.json', 1_284, 9, lin),
                              file('会议录音 0930.m4a', 6_803_211, 5, cheng, true),
                          ],
                      ],
                      [
                          '/mock-docs',
                          [
                              file('接口变更记录.md', 18_442, 20, cheng),
                              file('协议对照表.xlsx', 96_211, 14, lin),
                              file('架构图.png', 412_000, 30, cheng),
                          ],
                      ],
                      [
                          '/mock-release',
                          [
                              file('NapCatQQ-Desktop-3.3.0.msi', 61_337_600, 4, lin),
                              file('更新日志.txt', 3_120, 4, lin),
                          ],
                      ],
                  ]),
              }
            : { folders: [], files: new Map([['/', [file('行程表.xlsx', 22_011, 2, cheng)]]]) };
    trees.set(groupId, value);
    return value;
}

function locate(groupId: string, fileId: string): [string, GroupFile[], number] {
    for (const [folder, files] of tree(groupId).files) {
        const index = files.findIndex((entry) => entry.fileId === fileId);
        if (index >= 0) return [folder, files, index];
    }
    throw new Error('文件不存在或已被删除');
}
const folderFiles = (groupId: string, folder: string) => {
    const t = tree(groupId);
    if (!t.files.has(folder)) t.files.set(folder, []);
    return t.files.get(folder)!;
};
const syncCount = (groupId: string) => {
    const t = tree(groupId);
    for (const folder of t.folders) folder.fileCount = t.files.get(folder.folderId)?.length ?? 0;
};
const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

export const chatGroupFilesMock = {
    async list(groupId: string, folderId?: string | null): Promise<GroupFileListing> {
        await wait(220);
        const folder = folderId && folderId !== '/' ? folderId : '/';
        const t = tree(groupId);
        if (folder !== '/' && !t.folders.some((entry) => entry.folderId === folder))
            throw new Error('文件夹不存在');
        return {
            folders: folder === '/' ? t.folders.map((entry) => ({ ...entry })) : [],
            files: folderFiles(groupId, folder).map((entry) => ({ ...entry })),
            more: false,
            limit: 200,
            features: { renameFolder: true },
        };
    },
    async space(groupId: string): Promise<GroupFileSpace> {
        const t = tree(groupId);
        let used = 0;
        let count = 0;
        for (const files of t.files.values())
            for (const entry of files) {
                used += entry.size;
                count++;
            }
        return { fileCount: count, limitCount: 10000, usedSpace: used, totalSpace: 10 * 1024 ** 3 };
    },
    async act(groupId: string, action: GroupFileAction): Promise<void> {
        await wait(160);
        const t = tree(groupId);
        if (action.kind === 'deleteFile') {
            const [, files, index] = locate(groupId, action.fileId);
            files.splice(index, 1);
        } else if (action.kind === 'renameFile') {
            const [, files, index] = locate(groupId, action.fileId);
            files[index] = { ...files[index], name: action.name.trim() };
        } else if (action.kind === 'persist') {
            const [, files, index] = locate(groupId, action.fileId);
            files[index] = { ...files[index], deadTime: 0 };
        } else if (action.kind === 'moveFile') {
            const [, files, index] = locate(groupId, action.fileId);
            const [moved] = files.splice(index, 1);
            folderFiles(groupId, action.to).unshift(moved);
        } else if (action.kind === 'createFolder') {
            if (t.folders.some((entry) => entry.name === action.name.trim()))
                throw new Error('已有同名文件夹');
            t.folders.unshift({
                folderId: `/mock-${++serial}`,
                name: action.name.trim(),
                createTime: Math.floor(Date.now() / 1000),
                creator: '10001',
                creatorName: '预览账号',
                fileCount: 0,
            });
        } else if (action.kind === 'deleteFolder') {
            t.folders = t.folders.filter((entry) => entry.folderId !== action.folderId);
            t.files.delete(action.folderId);
        } else if (action.kind === 'renameFolder') {
            const folder = t.folders.find((entry) => entry.folderId === action.folderId);
            if (folder) folder.name = action.name.trim();
        }
        syncCount(groupId);
    },
    async transfer(
        requestId: string,
        name: string,
        size: number,
        stage: 'uploading' | 'downloading',
        onProgress?: (progress: DebugStreamProgress) => void,
    ): Promise<void> {
        cancelled.delete(requestId);
        for (let step = 1; step <= 10; step++) {
            await wait(180);
            if (cancelled.has(requestId)) {
                cancelled.delete(requestId);
                throw new Error('已取消');
            }
            onProgress?.({
                v: 1,
                request_id: requestId,
                stage,
                file_name: name,
                done_bytes: Math.round((size * step) / 10),
                total_bytes: size,
                done_chunks: step,
                total_chunks: 10,
            });
        }
    },
    addUpload(groupId: string, folderId: string, name: string, size: number): void {
        folderFiles(groupId, folderId || '/').unshift(file(name, size, 0, ['10001', '预览账号']));
        syncCount(groupId);
    },
    sizeOf(groupId: string, fileId: string): number {
        try {
            const [, files, index] = locate(groupId, fileId);
            return files[index].size;
        } catch {
            return 1_000_000;
        }
    },
    cancel(requestId: string): void {
        cancelled.add(requestId);
    },
};
