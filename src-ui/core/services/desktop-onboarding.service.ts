// Desktop 新手引导 IPC。
// 命令：get / start / skip / complete / reopen_desktop_onboarding。

import { invoke, isTauri } from '../ipc/transport';
import type { DesktopOnboardingPayload } from '../ipc/generated/DesktopOnboardingPayload';
import type { DesktopOnboardingState } from '../ipc/generated/DesktopOnboardingState';
import type { OnboardingStatus } from '../ipc/generated/OnboardingStatus';

export type { DesktopOnboardingPayload, DesktopOnboardingState, OnboardingStatus };

const MOCK_STATE: DesktopOnboardingState = {
    version: 1,
    status: 'skipped',
    decidedAt: null,
    lastOpenedAt: null,
    completedStepIds: [],
};

const MOCK_PAYLOAD: DesktopOnboardingPayload = {
    state: MOCK_STATE,
    shouldPromptChoice: false,
    schemaVersion: 1,
};

function mockWith(
    status: OnboardingStatus,
    shouldPromptChoice: boolean,
): DesktopOnboardingPayload {
    const now = new Date().toISOString();
    return {
        schemaVersion: 1,
        shouldPromptChoice,
        state: {
            version: 1,
            status,
            decidedAt: status === 'pending' ? null : now,
            lastOpenedAt: status === 'pending' ? null : now,
            completedStepIds: [],
        },
    };
}

export const desktopOnboardingService = {
    get: async (): Promise<DesktopOnboardingPayload> => {
        if (!isTauri) return { ...MOCK_PAYLOAD };
        return invoke<DesktopOnboardingPayload>('get_desktop_onboarding');
    },

    start: async (): Promise<DesktopOnboardingPayload> => {
        if (!isTauri) return mockWith('active', false);
        return invoke<DesktopOnboardingPayload>('start_desktop_onboarding');
    },

    skip: async (): Promise<DesktopOnboardingPayload> => {
        if (!isTauri) return mockWith('skipped', false);
        return invoke<DesktopOnboardingPayload>('skip_desktop_onboarding');
    },

    complete: async (completedStepIds?: string[]): Promise<DesktopOnboardingPayload> => {
        if (!isTauri) {
            const p = mockWith('completed', false);
            return {
                ...p,
                state: { ...p.state, completedStepIds: completedStepIds ?? [] },
            };
        }
        return invoke<DesktopOnboardingPayload>('complete_desktop_onboarding', {
            completedStepIds: completedStepIds ?? null,
        });
    },

    reopen: async (): Promise<DesktopOnboardingPayload> => {
        if (!isTauri) return mockWith('completed', false);
        return invoke<DesktopOnboardingPayload>('reopen_desktop_onboarding');
    },
};
