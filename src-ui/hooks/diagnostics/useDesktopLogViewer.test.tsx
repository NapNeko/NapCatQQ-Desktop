// @vitest-environment jsdom
import { act, cleanup, renderHook } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { useDesktopLogViewer } from './useDesktopLogViewer';

const harness = vi.hoisted(() => ({
    openLogLocation: vi.fn(),
    logDirectory: vi.fn(),
    writeText: vi.fn(),
    error: vi.fn(),
    info: vi.fn(),
}));
vi.mock('../../core/services/desktop.service', () => ({ desktopLogService: harness }));
vi.mock('./useDesktopLogStream', () => ({
    useDesktopLogStream: () => ({ logs: [], loading: false, error: null, reload: vi.fn() }),
}));
vi.mock('../ui/pushErrorBar', () => ({ pushErrorBar: harness.error }));
vi.mock('../ui/globalInfoBarStore', () => ({ pushInfoBar: harness.info }));

beforeEach(() => {
    vi.clearAllMocks();
    harness.openLogLocation.mockResolvedValue('/data/logs/desktop/session.log');
    harness.writeText.mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { userAgent: 'Linux', clipboard: { writeText: harness.writeText } });
});
afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
});

describe('desktop log location', () => {
    it('preserves Windows native-open error handling', async () => {
        vi.stubGlobal('navigator', { userAgent: 'Windows NT' });
        const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
        harness.openLogLocation.mockRejectedValueOnce(
            new Error('文件管理器退出失败: exit code: 1'),
        );
        const { result } = renderHook(() => useDesktopLogViewer(false));
        await act(async () => result.current.onOpenLocation());
        expect(harness.openLogLocation).toHaveBeenCalledTimes(1);
        expect(harness.error).not.toHaveBeenCalled();
        expect(warning).toHaveBeenCalled();
    });
    it('shows a visible fallback when the system file manager fails', async () => {
        harness.openLogLocation.mockRejectedValueOnce(new Error('xdg-open exit 3'));
        const { result } = renderHook(() => useDesktopLogViewer(false));
        await act(async () => result.current.onOpenLocation());
        expect(harness.error).toHaveBeenCalledWith(
            expect.objectContaining({
                title: '打开日志目录失败',
                raw: 'xdg-open exit 3',
                content: expect.stringContaining('复制目录'),
            }),
        );
        expect(result.current.opening).toBe(false);
    });

    it.each([
        '/home/alex/.local/share/NapCatQQ Desktop/logs/desktop',
        'C:\\ProgramData\\NapCatQQ Desktop\\logs\\desktop',
    ])('copies the exact directory without invoking a file manager: %s', async (path) => {
        harness.logDirectory.mockResolvedValueOnce(path);
        const { result } = renderHook(() => useDesktopLogViewer(false));
        await act(async () => result.current.onCopyLocation());
        expect(harness.writeText).toHaveBeenCalledWith(path);
        expect(harness.openLogLocation).not.toHaveBeenCalled();
        expect(result.current.copyingLocation).toBe(false);
    });
});
