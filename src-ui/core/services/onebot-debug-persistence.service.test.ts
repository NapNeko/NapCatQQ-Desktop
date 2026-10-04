import { beforeEach, describe, expect, it, vi } from 'vitest';
import { QueryClient } from '@tanstack/react-query';
import { addFolder } from '../domain/debug/collectionsOps';

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('../ipc/transport', async original => ({ ...await original<typeof import('../ipc/transport')>(), isTauri: true, invoke: native.invoke }));
beforeEach(() => { native.invoke.mockReset(); });

describe('debug snapshots after configuration restore', () => {
    it('saves an old workspace with its original revision even after another workspace is read', async () => {
        const { onebotDebugService } = await import('./onebot-debug.service');
        const workspace = { version: 2, tabs: [], active_tab: null, closed_tabs: [], selected_bot: 'old', channel_choice: {}, layout: { left_collapsed: false, right_collapsed: false, left_width: 0, right_width: 0, right_view: 'chat' }, recent_actions: [] };
        native.invoke.mockResolvedValueOnce({ workspace, revision: 0 });
        const old = await onebotDebugService.workspace();
        native.invoke.mockResolvedValueOnce({ workspace: { ...workspace, selected_bot: 'imported' }, revision: 1 });
        await onebotDebugService.workspace();
        native.invoke.mockResolvedValueOnce(undefined);
        await onebotDebugService.saveWorkspace({ ...old, selected_bot: 'old edit' });
        expect(native.invoke).toHaveBeenLastCalledWith('onebot_debug_save_workspace', { workspace: { ...workspace, selected_bot: 'old edit' }, revision: 0 });
    });

    it('preserves the collection revision through real edits and React Query structural sharing', async () => {
        const { onebotDebugService } = await import('./onebot-debug.service');
        native.invoke.mockResolvedValueOnce({ collections: { version: 1, folders: [], requests: [] }, revision: 3 });
        const old = await onebotDebugService.collections();
        const client = new QueryClient();
        const next = addFolder(old, '新收藏').next;
        const shared = client.setQueryData(['collections'], next)!;
        native.invoke.mockResolvedValueOnce({ collections: { version: 1, folders: [], requests: [] }, revision: 4 });
        await onebotDebugService.collections();
        native.invoke.mockResolvedValueOnce(undefined);
        await onebotDebugService.saveCollections(shared);
        expect(native.invoke).toHaveBeenLastCalledWith('onebot_debug_save_collections', {
            collections: { version: 1, folders: next.folders, requests: [] }, revision: 3,
        });
        client.clear();
    });
});
