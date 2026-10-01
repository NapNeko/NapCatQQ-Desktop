import { describe, expect, it } from 'vitest';
import {
    bytesText,
    isLocalFileToken,
    localFilesInParams,
    localFileTokenFor,
    needsStreamCall,
    progressText,
    streamChannelBlocker,
    LOCAL_FILE_PREFIX,
    STREAM_CHANNEL_REASON,
} from './streamActions';
import type { DebugStreamProgress } from '../../ipc/generated/debug/DebugStreamProgress';

describe('本机文件占位', () => {
    const path = 'C:\\tmp\\报告 a.png';
    const token = localFileTokenFor(path);

    it('占位是前缀 + 路径原文，判断能认回来', () => {
        expect(token.startsWith(LOCAL_FILE_PREFIX)).toBe(true);
        expect(isLocalFileToken(token)).toBe(true);
        expect(isLocalFileToken('http://example.com/a.png')).toBe(false);
        expect(isLocalFileToken(LOCAL_FILE_PREFIX)).toBe(false);
        expect(isLocalFileToken(42)).toBe(false);
    });

    it('递归找齐占位：嵌在消息段里的也算，同一份文件只算一次', () => {
        const params = {
            file: token,
            message: [
                { type: 'image', data: { file: token } },
                { type: 'text', data: { text: token } },
            ],
            url: 'http://example.com/x',
        };
        const files = localFilesInParams(params);
        expect(files).toEqual([{ path, token }]);
    });
});

describe('要不要走流式命令', () => {
    it('分块下载、或带本机文件占位', () => {
        expect(needsStreamCall('download_file_stream', {})).toBe(true);
        expect(needsStreamCall('test_download_stream', {})).toBe(true);
        expect(needsStreamCall('send_msg', { file: localFileTokenFor('C:/a.png') })).toBe(true);
        expect(needsStreamCall('upload_file_stream', {})).toBe(false);
        expect(needsStreamCall('clean_stream_temp_file', {})).toBe(false);
        expect(needsStreamCall('send_msg', { message: 'hi' })).toBe(false);
    });

    it('通道上前端就能确定的只有点名的 HTTP / WS', () => {
        expect(streamChannelBlocker('download_file_stream', 0, { kind: 'http', name: 'h' })).toBe(STREAM_CHANNEL_REASON);
        expect(streamChannelBlocker('download_file_stream', 0, { kind: 'internal' })).toBeNull();
        expect(streamChannelBlocker('download_file_stream', 0, { kind: 'auto' })).toBeNull();
        expect(streamChannelBlocker('upload_file_stream', 1, { kind: 'ws', name: 'w' })).toBe(STREAM_CHANNEL_REASON);
        expect(streamChannelBlocker('upload_file_stream', 0, { kind: 'ws', name: 'w' })).toBeNull();
        expect(streamChannelBlocker('send_msg', 1, { kind: 'http', name: 'h' })).toBeNull();
    });
});

describe('进度文案', () => {
    const beat = (patch: Partial<DebugStreamProgress>): DebugStreamProgress => ({
        v: 1,
        request_id: 'r',
        stage: 'uploading',
        file_name: 'a.png',
        done_bytes: 0,
        total_bytes: null,
        done_chunks: 0,
        total_chunks: null,
        ...patch,
    });

    it('字节数的人的读法', () => {
        expect(bytesText(0)).toBe('0 B');
        expect(bytesText(1023)).toBe('1023 B');
        expect(bytesText(1536)).toBe('1.5 KiB');
        expect(bytesText(48 * 1024 * 1024)).toBe('48.0 MiB');
        expect(bytesText(2 * 1024 ** 3)).toBe('2.0 GiB');
    });

    it('总量已知带百分比，未知只写已传量和块数', () => {
        expect(progressText(beat({ done_bytes: 12 * 1024, total_bytes: 48 * 1024 }))).toBe('上传中 12.0 KiB / 48.0 KiB（25%）');
        expect(progressText(beat({ stage: 'downloading', done_bytes: 1024, done_chunks: 3 }))).toBe('下载中 1.0 KiB，3 块');
        expect(progressText(beat({ stage: 'downloading' }))).toBe('下载中 0 B');
        expect(progressText(beat({ stage: 'reading' }))).toBe('读本机文件 a.png…');
        expect(progressText(beat({ stage: 'calling' }))).toBe('正在调用…');
    });
});
