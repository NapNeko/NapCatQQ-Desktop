import { describe, expect, it } from 'vitest';
import { DEMO_REMOTE_HOST_ID, TOUR_IDS, type TourTargetId } from './tourIds';
import {
    BOT_CREATE_STEPS,
    FULL_FRAMEWORK_TOUR_STEPS,
    LOCAL_FRAMEWORK_STEPS,
    REMOTE_DEMO_STEPS,
    type FrameworkTourStep,
} from './frameworkTourSteps';

const ALL_TARGETS = new Set<string>(Object.values(TOUR_IDS));
const PHASES: Record<string, true> = { local: true, remote: true, bots: true };

function assertShape(steps: readonly FrameworkTourStep[]) {
    for (const step of steps) {
        expect(step.id).toBeTruthy();
        expect(PHASES[step.phase]).toBe(true);
        expect(ALL_TARGETS.has(step.target)).toBe(true);
        expect(step.title.length).toBeGreaterThan(0);
        expect(step.body.length).toBeGreaterThan(0);
    }
}

describe('LOCAL_FRAMEWORK_STEPS', () => {
    it('每一步都指向真实 TOUR_IDS 锚点且字段完整', () => {
        assertShape(LOCAL_FRAMEWORK_STEPS);
    });

    it('本机阶段全部选中 local 主机', () => {
        expect(LOCAL_FRAMEWORK_STEPS.length).toBeGreaterThan(0);
        for (const step of LOCAL_FRAMEWORK_STEPS) {
            expect(step.phase).toBe('local');
            expect(step.selectHostId).toBe('local');
        }
    });

    it('覆盖两个协议端 + QQ 底座行', () => {
        const targets = LOCAL_FRAMEWORK_STEPS.map((s) => s.target);
        expect(targets).toEqual(
            expect.arrayContaining([
                TOUR_IDS.groupFramework,
                TOUR_IDS.rowNapcat,
                TOUR_IDS.rowSnowluma,
                TOUR_IDS.rowQq,
            ]),
        );
    });
});

describe('REMOTE_DEMO_STEPS', () => {
    it('每一步都指向真实 TOUR_IDS 锚点且字段完整', () => {
        assertShape(REMOTE_DEMO_STEPS);
    });

    it('远端阶段全部选中演示远端主机（不落真档案）', () => {
        expect(REMOTE_DEMO_STEPS.length).toBeGreaterThan(0);
        for (const step of REMOTE_DEMO_STEPS) {
            expect(step.phase).toBe('remote');
            expect(step.selectHostId).toBe(DEMO_REMOTE_HOST_ID);
        }
    });

    it('演示远端锚点与切换 tab 一致，且提到 noVNC（SL 专属依赖）', () => {
        expect(REMOTE_DEMO_STEPS[0]!.target).toBe(TOUR_IDS.hostTabDemoRemote);
        const targets = REMOTE_DEMO_STEPS.map((s) => s.target);
        expect(targets).toContain(TOUR_IDS.rowNovnc);
        expect(targets).toContain(TOUR_IDS.groupRuntime);
    });
});

describe('BOT_CREATE_STEPS', () => {
    it('每一步都指向真实 TOUR_IDS 锚点且字段完整', () => {
        assertShape(BOT_CREATE_STEPS);
    });

    it('bots 阶段不选主机', () => {
        expect(BOT_CREATE_STEPS.length).toBeGreaterThan(0);
        for (const step of BOT_CREATE_STEPS) {
            expect(step.phase).toBe('bots');
            expect(step.selectHostId).toBeUndefined();
        }
    });

    it('流程从侧栏导航走到保存动作，最后回列表', () => {
        const targets: TourTargetId[] = BOT_CREATE_STEPS.map((s) => s.target);
        expect(targets[0]).toBe(TOUR_IDS.navBots);
        expect(targets).toEqual(
            expect.arrayContaining([
                TOUR_IDS.botConfigHeader,
                TOUR_IDS.botIdentitySection,
                TOUR_IDS.botRuntimeSection,
                TOUR_IDS.botConnectionsBody,
                TOUR_IDS.botSaveActions,
            ]),
        );
        expect(targets[targets.length - 1]).toBe(TOUR_IDS.botListHeader);
    });
});

describe('FULL_FRAMEWORK_TOUR_STEPS', () => {
    it('是本机段 + 远端段的按序拼接，不含 bots 段', () => {
        expect(FULL_FRAMEWORK_TOUR_STEPS).toEqual([...LOCAL_FRAMEWORK_STEPS, ...REMOTE_DEMO_STEPS]);
        expect(FULL_FRAMEWORK_TOUR_STEPS.every((s) => s.phase !== 'bots')).toBe(true);
    });

    it('全量步骤 id 不重复', () => {
        const ids = FULL_FRAMEWORK_TOUR_STEPS.map((s) => s.id);
        expect(new Set(ids).size).toBe(ids.length);
    });
});
