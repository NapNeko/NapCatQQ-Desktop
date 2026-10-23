import { beforeEach, describe, expect, it, vi } from 'vitest';
import { componentService } from './component.service';

const native = vi.hoisted(() => ({ invoke: vi.fn() }));
vi.mock('../ipc/transport', () => ({ isTauri: true, invoke: native.invoke }));

beforeEach(() => native.invoke.mockReset().mockResolvedValue('task'));

describe('component action sudo transport', () => {
    it('sends the password only with the requested action and explicit remember choice', async () => {
        await componentService.runComponentAction(
            'qq',
            'remote:one',
            'update',
            'task',
            undefined,
            'temporary',
            false,
        );
        expect(native.invoke).toHaveBeenCalledWith('run_component_action', {
            componentId: 'qq',
            hostId: 'remote:one',
            kind: 'update',
            taskId: 'task',
            snowlumaLinuxPackage: null,
            sudoPassword: 'temporary',
            rememberSudo: false,
        });
    });

    it('omits credentials for ordinary component actions', async () => {
        await componentService.runComponentAction('qq', 'remote:one', 'update', 'task');
        expect(native.invoke.mock.calls[0][1]).not.toHaveProperty('sudoPassword');
        expect(native.invoke.mock.calls[0][1]).not.toHaveProperty('rememberSudo');
    });
});
