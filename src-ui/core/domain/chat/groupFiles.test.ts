import { describe, expect, it } from 'vitest';
import {
    canChangeFile,
    canManageFolders,
    downloadFileName,
    expiryLabel,
    fileKind,
    fileSegmentName,
    fileSegmentSource,
    groupFileRows,
    transferPercent,
} from './groupFiles';
import type { GroupFile } from '../../ipc/generated/chat/GroupFile';
import type { GroupFolder } from '../../ipc/generated/chat/GroupFolder';

const file = (
    name: string,
    size: number,
    uploadTime: number,
    uploader = '1',
    uploaderName = '甲',
): GroupFile => ({
    fileId: name,
    name,
    busid: 102,
    size,
    uploadTime,
    deadTime: 0,
    modifyTime: 0,
    downloadTimes: 0,
    uploader,
    uploaderName,
});
const folder = (name: string, createTime: number, fileCount = 0): GroupFolder => ({
    folderId: `/${name}`,
    name,
    createTime,
    creator: '1',
    creatorName: '乙',
    fileCount,
});

describe('group file rows', () => {
    const folders = [folder('b 资料', 1, 3), folder('a 图片', 5, 9)];
    const files = [file('文件10.txt', 5, 30), file('文件2.txt', 50, 10, '2', '丙')];

    it('keeps folders first and sorts each part by the chosen key', () => {
        expect(
            groupFileRows(folders, files, 'time', '').map((r) =>
                r.kind === 'folder' ? r.folder.name : r.file.name,
            ),
        ).toEqual(['a 图片', 'b 资料', '文件10.txt', '文件2.txt']);
        // 名称排序按数字自然序
        expect(
            groupFileRows(folders, files, 'name', '').map((r) =>
                r.kind === 'folder' ? r.folder.name : r.file.name,
            ),
        ).toEqual(['a 图片', 'b 资料', '文件2.txt', '文件10.txt']);
        expect(
            groupFileRows([], files, 'size', '').map((r) => r.kind === 'file' && r.file.size),
        ).toEqual([50, 5]);
    });

    it('filters by name or uploader', () => {
        expect(
            groupFileRows(folders, files, 'time', '丙').map(
                (r) => r.kind === 'file' && r.file.name,
            ),
        ).toEqual(['文件2.txt']);
        expect(groupFileRows(folders, files, 'time', '图片')).toHaveLength(1);
    });
});

describe('group file labels and permissions', () => {
    it('describes temporary file expiry', () => {
        expect(expiryLabel(0, 100)).toBe('');
        expect(expiryLabel(50, 100)).toBe('已过期');
        expect(expiryLabel(100 + 3600, 100)).toBe('今天过期');
        expect(expiryLabel(100 + 86400 * 3, 100)).toBe('3 天后过期');
    });

    it('lets managers and uploaders change files and leaves unknown roles to upstream', () => {
        const mine = file('a', 1, 1, '99');
        expect(canChangeFile(mine, '99', 'member')).toBe(true);
        expect(canChangeFile(file('b', 1, 1, '7'), '99', 'member')).toBe(false);
        expect(canChangeFile(file('b', 1, 1, '7'), '99', 'admin')).toBe(true);
        expect(canChangeFile(file('b', 1, 1, '7'), '99', null)).toBe(true);
        expect(canManageFolders('member')).toBe(false);
        expect(canManageFolders('owner')).toBe(true);
    });

    it('maps names to icon kinds and progress to percent', () => {
        expect(fileKind('a.ZIP')).toBe('archive');
        expect(fileKind('报告.pdf')).toBe('doc');
        expect(fileKind('noext')).toBe('other');
        expect(transferPercent({ done_bytes: 5, total_bytes: 10 })).toBe(50);
        expect(transferPercent({ done_bytes: 5, total_bytes: null })).toBeNull();
    });
});

describe('file segment source', () => {
    it('needs a file id and carries group busid or private file hash', () => {
        expect(fileSegmentSource({ file: 'a.txt' }, { type: 'group', id: '3' })).toBeNull();
        expect(fileSegmentSource({ file_id: 'x', busid: 102 }, { type: 'group', id: '3' })).toEqual(
            { kind: 'group', groupId: '3', fileId: 'x', busid: 102 },
        );
        expect(
            fileSegmentSource({ file_id: 'x', file_hash: 'h' }, { type: 'private', id: '8' }),
        ).toEqual({ kind: 'private', userId: '8', fileId: 'x', fileHash: 'h' });
    });

    it('accepts legacy SnowLuma ids for group and friend file messages', () => {
        const data = { id: 'legacy-id', name: '资料.zip', size: 1024, file_hash: 'hash' };
        expect(fileSegmentSource(data, { type: 'group', id: '3' })).toEqual({
            kind: 'group',
            groupId: '3',
            fileId: 'legacy-id',
            busid: null,
        });
        expect(fileSegmentSource(data, { type: 'private', id: '8' })).toEqual({
            kind: 'private',
            userId: '8',
            fileId: 'legacy-id',
            fileHash: 'hash',
        });
        expect(
            fileSegmentSource(
                { fileId: 'camel-id', fileHash: 'camel-hash' },
                { type: 'private', id: '8' },
            ),
        ).toEqual({ kind: 'private', userId: '8', fileId: 'camel-id', fileHash: 'camel-hash' });
    });

    it('preserves a download URL without mistaking a filename or local path for an id', () => {
        const url = 'https://cdn.example/file?token=download';
        const session = { type: 'group' as const, id: '3' };
        expect(
            fileSegmentSource(
                { file_id: 'canonical', id: 'legacy', file: '资料.zip', url },
                session,
            ),
        ).toEqual({ kind: 'group', groupId: '3', fileId: 'canonical', busid: null, url });
        expect(fileSegmentSource({ name: '资料.zip', url }, session)).toEqual({ kind: 'url', url });
        expect(fileSegmentSource({ file: url }, session)).toEqual({ kind: 'url', url });
        for (const value of [
            '资料.zip',
            'C:\\Downloads\\资料.zip',
            'file:///tmp/a.txt',
            'ncd-local-file:///tmp/a.txt',
            'javascript:alert(1)',
            'https://user:pass@cdn.example/a',
        ]) {
            expect(fileSegmentSource({ file: value }, session)).toBeNull();
            expect(fileSegmentSource({ url: value }, session)).toBeNull();
        }
    });
});

describe('file download names', () => {
    it('derives URL-only filenames from the decoded path without leaking signed query strings', () => {
        expect(fileSegmentName({ file: 'https://cdn.example/report.pdf?token=secret#part' })).toBe(
            'report.pdf',
        );
        expect(
            fileSegmentName({ url: 'https://cdn.example/%E6%8A%A5%E5%91%8A%201.pdf?token=secret' }),
        ).toBe('报告 1.pdf');
        expect(fileSegmentName({ file: 'https://cdn.example/report%3A%3F%2F.pdf' })).toBe(
            'report___.pdf',
        );
        expect(fileSegmentName({ url: 'https://cdn.example/?token=secret' })).toBe('文件');
        expect(fileSegmentName({ url: 'https://cdn.example/%E6.pdf' })).toBe('%E6.pdf');
    });

    it('keeps protocol filenames for display and makes only the save suggestion Windows-safe', () => {
        expect(fileSegmentName({ name: '报告:最终.pdf', url: 'https://cdn.example/opaque' })).toBe(
            '报告:最终.pdf',
        );
        expect(fileSegmentName({ file_name: '资料.zip', file: 'https://cdn.example/opaque' })).toBe(
            '资料.zip',
        );
        expect(downloadFileName('报告:最终?.pdf')).toBe('报告_最终_.pdf');
        expect(downloadFileName('folder/name\u0000.txt. ')).toBe('folder_name_.txt');
        expect(downloadFileName('CON.txt')).toBe('_CON.txt');
        expect(downloadFileName('..')).toBe('文件');
    });
});
