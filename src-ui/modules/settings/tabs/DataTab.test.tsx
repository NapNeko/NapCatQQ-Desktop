// @vitest-environment jsdom
import { cleanup, fireEvent, render, screen, waitFor } from '@testing-library/react';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DataTab } from './DataTab';

const harness = vi.hoisted(() => ({ writeText: vi.fn(), error: vi.fn(), info: vi.fn() }));
vi.mock('../../../hooks/ui/pushErrorBar', () => ({ pushErrorBar: harness.error }));
vi.mock('../../../hooks/ui/globalInfoBarStore', () => ({ pushInfoBar: harness.info }));
vi.mock('../../../hooks/preferences/useConfigTransfer', () => ({
    useConfigTransfer: () => ({ pendingFrameworks: [], canRetryPreferences: false }),
}));
vi.mock('../ConfigImportDialog', () => ({ ConfigImportDialog: () => null }));
vi.mock('../DataRootMigrateDialog', () => ({ DataRootMigrateDialog: () => null }));
vi.mock('../CacheCleanupDialog', () => ({ CacheCleanupDialog: () => null }));

beforeEach(() => {
    vi.clearAllMocks();
    harness.writeText.mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { userAgent: 'Linux', clipboard: { writeText: harness.writeText } });
});
afterEach(() => {
    cleanup();
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
});

describe('data directory controls', () => {
    it.each(['/home/alex/NapCatQQ Desktop/数据', 'C:\\ProgramData\\NapCatQQ Desktop'])(
        'copies the exact data directory without launching a file manager: %s',
        async (dataRoot) => {
            const open = vi.fn();
            render(<DataTab dataRoot={dataRoot} onOpenDataDir={open} isOpeningDir={false} />);
            fireEvent.click(screen.getByRole('button', { name: '复制目录' }));
            await waitFor(() =>
                expect(harness.info).toHaveBeenCalledWith(
                    expect.objectContaining({ content: dataRoot }),
                ),
            );
            expect(harness.writeText).toHaveBeenCalledWith(dataRoot);
            expect(open).not.toHaveBeenCalled();
        },
    );

    it('shows a Linux file manager error and keeps the copy fallback available', async () => {
        const open = vi.fn().mockRejectedValue(new Error('xdg-open exit 3'));
        render(<DataTab dataRoot="/data" onOpenDataDir={open} isOpeningDir={false} />);
        fireEvent.click(screen.getByRole('button', { name: '打开', exact: true }));
        await waitFor(() =>
            expect(harness.error).toHaveBeenCalledWith(
                expect.objectContaining({ title: '打开数据目录失败', raw: 'xdg-open exit 3' }),
            ),
        );
        expect(screen.getByRole('button', { name: '复制目录' })).toBeEnabled();
    });

    it('preserves Windows native-open error handling', async () => {
        vi.stubGlobal('navigator', { userAgent: 'Windows NT' });
        const warning = vi.spyOn(console, 'warn').mockImplementation(() => {});
        const open = vi.fn().mockRejectedValue(new Error('explorer exit 1'));
        render(<DataTab dataRoot="C:\\Data" onOpenDataDir={open} isOpeningDir={false} />);
        fireEvent.click(screen.getByRole('button', { name: '打开', exact: true }));
        await waitFor(() => expect(warning).toHaveBeenCalled());
        expect(harness.error).not.toHaveBeenCalled();
    });

    it('shows clipboard failures and disables copying an unavailable directory', async () => {
        harness.writeText.mockRejectedValueOnce(new Error('clipboard denied'));
        const { rerender } = render(
            <DataTab dataRoot="/data" onOpenDataDir={vi.fn()} isOpeningDir={false} />,
        );
        fireEvent.click(screen.getByRole('button', { name: '复制目录' }));
        await waitFor(() =>
            expect(harness.error).toHaveBeenCalledWith(
                expect.objectContaining({ title: '复制数据目录失败' }),
            ),
        );
        rerender(<DataTab dataRoot="—" onOpenDataDir={vi.fn()} isOpeningDir={false} />);
        expect(screen.getByRole('button', { name: '复制目录' })).toBeDisabled();
    });
});
