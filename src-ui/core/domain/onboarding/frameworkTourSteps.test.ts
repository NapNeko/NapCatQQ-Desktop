import { describe, expect, it } from 'vitest';
import { DEMO_REMOTE_HOST_ID } from './tourIds';
import {
    BOT_CREATE_STEPS,
    FULL_FRAMEWORK_TOUR_STEPS,
    LOCAL_FRAMEWORK_STEPS,
    REMOTE_DEMO_STEPS,
} from './frameworkTourSteps';

describe('引导流程', () => {
    it('演示只选择本机或演示主机，创建 Bot 不切换主机', () => {
        expect(new Set(LOCAL_FRAMEWORK_STEPS.map((step) => step.selectHostId))).toEqual(
            new Set(['local']),
        );
        expect(new Set(REMOTE_DEMO_STEPS.map((step) => step.selectHostId))).toEqual(
            new Set([DEMO_REMOTE_HOST_ID]),
        );
        expect(BOT_CREATE_STEPS.every((step) => step.selectHostId === undefined)).toBe(true);
    });

    it('步骤标识唯一，恢复进度时不会跳到同名步骤', () => {
        const ids = [...FULL_FRAMEWORK_TOUR_STEPS, ...BOT_CREATE_STEPS].map((step) => step.id);
        expect(new Set(ids).size).toBe(ids.length);
    });
});
