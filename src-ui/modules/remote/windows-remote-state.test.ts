import { describe, expect, it } from 'vitest';
import type { RemoteWindowsInfo } from '../../core/ipc/generated/domain/RemoteWindowsInfo';
import {
    canInstallWindowsAgent,
    registerWindowsSessionCommand,
    windowsManagementLabel,
    windowsSessionState,
    windowsWorkerUpdatePending,
} from './windows-remote-state';

function host(): RemoteWindowsInfo {
    return {
        v: 1,
        osVersion: 'Windows 11',
        architecture: 'AMD64',
        userSid: 'S-1-5-21-10',
        username: 'PC\\远端',
        sessionId: 0,
        elevated: true,
        homeDir: 'D:\\Users\\远端',
        programData: 'D:\\ProgramData',
        tempDir: 'D:\\Temp',
        managedRoot: 'D:\\ProgramData\\NapCatQQ Desktop\\remote',
        userRoot: 'D:\\ProgramData\\NapCatQQ Desktop\\remote\\users\\abc',
        agent: {
            v: 1,
            version: '0.1.0',
            serviceRunning: true,
            serviceIdentity: 'NT AUTHORITY\\LOCAL SERVICE',
            capabilities: [],
            sessions: [],
        },
    };
}

describe('Windows remote management gates', () => {
    it('does not reuse a cached running service or desktop session after disconnection', () => {
        const info = host();
        info.agent!.sessions = [
            {
                userSid: info.userSid,
                sessionId: 2,
                workerReady: true,
                workerUpdatePending: false,
                locked: false,
            },
        ];
        expect(windowsSessionState(info, false)).toBe('offline');
        expect(windowsManagementLabel(info, false)).toBe('未连接');
        expect(canInstallWindowsAgent(info, false)).toBe(false);
    });

    it('requires a worker belonging to the SSH account, independently of the SSH logon session', () => {
        const info = host();
        info.sessionId = 3;
        info.agent!.sessions = [
            {
                userSid: 'S-1-5-21-other',
                sessionId: 3,
                workerReady: true,
                workerUpdatePending: false,
                locked: false,
            },
        ];
        expect(windowsSessionState(info, true)).toBe('waiting');
        info.agent!.sessions.push({
            userSid: info.userSid,
            sessionId: 4,
            workerReady: false,
            workerUpdatePending: false,
            locked: false,
        });
        expect(windowsSessionState(info, true)).toBe('waiting');
        info.agent!.sessions[1].workerReady = true;
        expect(windowsSessionState(info, true)).toBe('ready');
    });

    it('exposes locked and ambiguous sessions instead of advertising QR readiness', () => {
        const info = host();
        info.agent!.sessions = [
            {
                userSid: info.userSid,
                sessionId: 2,
                workerReady: true,
                workerUpdatePending: false,
                locked: true,
            },
        ];
        expect(windowsSessionState(info, true)).toBe('locked');
        info.agent!.sessions.push({
            userSid: info.userSid,
            sessionId: 3,
            workerReady: true,
            workerUpdatePending: false,
            locked: false,
        });
        expect(windowsSessionState(info, true)).toBe('multiple');
    });

    it('shows a pending worker update without stopping or hiding a running owner session', () => {
        const info = host();
        info.agent!.sessions = [
            {
                userSid: 'S-1-5-21-other',
                sessionId: 2,
                workerReady: true,
                workerUpdatePending: true,
                locked: false,
            },
        ];
        expect(windowsWorkerUpdatePending(info, true)).toBe(false);
        info.agent!.sessions[0].userSid = info.userSid;
        expect(windowsWorkerUpdatePending(info, true)).toBe(true);
        expect(windowsSessionState(info, true)).toBe('ready');
        expect(windowsWorkerUpdatePending(info, false)).toBe(false);
        info.agent!.serviceRunning = false;
        expect(windowsWorkerUpdatePending(info, true)).toBe(false);
    });

    it('offers service installation only to a connected elevated account without a running service', () => {
        const info = host();
        info.agent = null;
        expect(canInstallWindowsAgent(info, true)).toBe(true);
        info.elevated = false;
        expect(canInstallWindowsAgent(info, true)).toBe(false);
        expect(windowsSessionState(info, true)).toBe('agent_unavailable');
        expect(canInstallWindowsAgent(null, true)).toBe(false);
    });

    it('binds the worker using the machine agent root and quotes target paths literally', () => {
        const info = host();
        info.managedRoot = "E:\\NapCat's 管理\\remote\\";
        const command = registerWindowsSessionCommand(info);
        expect(command).toBe(
            "& 'E:\\NapCat''s 管理\\remote\\agent\\current\\ncd-agent.exe' register-session --root 'E:\\NapCat''s 管理\\remote'",
        );
        expect(command).not.toContain(info.userRoot);
    });

    it('allows redeployment of a running service only while connected as administrator', () => {
        const info = host();
        expect(canInstallWindowsAgent(info, true)).toBe(false);
        expect(canInstallWindowsAgent(info, true, true)).toBe(true);
        expect(canInstallWindowsAgent(info, false, true)).toBe(false);
        info.elevated = false;
        expect(canInstallWindowsAgent(info, true, true)).toBe(false);
    });
});
