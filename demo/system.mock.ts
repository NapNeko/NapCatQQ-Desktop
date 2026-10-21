import type {
  CacheCategory, CacheCleanupResult, CacheScan, ComponentDetectResult,
  ComponentId, DependencyNode, DependencyPlan, ProgressEvent, RequirementPhase, StepKind,
} from '@desktop/core/ipc/types';
import { mockComponentCatalog, mockDependencyPlan, mockDetect } from '@desktop/core/ipc/mock/component.mock';
import { mockBackendSettings } from '@desktop/core/ipc/mock/settings.mock';
import { componentService } from '@desktop/core/services/component.service';
import { settingsService, type BackendSettings } from '@desktop/core/services/settings.service';
import { networkService } from '@desktop/core/services/network.service';
import { cacheCleanupService } from '@desktop/core/services/cache-cleanup.service';
import { validateNetworkSettings } from '@desktop/core/domain/settings/network';
import { defaultAppUiPreferencesFromPrefs } from '@desktop/core/domain/settings/ui-preferences-bridge';
import { preferencesStore } from '@desktop/core/domain/settings/preferencesStore';
import { featureTogglesStore } from '@desktop/hooks/preferences/featureTogglesStore';
import { componentActionStore } from '@desktop/hooks/components/componentActionStore';
import { deploymentTaskStore } from '@desktop/hooks/task-queue/deploymentTaskStore';
import { emitMockEvent } from './events.mock';

type SystemLessonId = 'components' | 'settings' | 'network' | 'backup';
type Installed = NonNullable<ComponentDetectResult['detected']>;
const baselineSettings = structuredClone(mockBackendSettings);
const installed = new Map<string, Installed | null>();
const tasks = new Map<string, ReturnType<typeof setInterval>>();
const versions: Partial<Record<ComponentId, string>> = {
  qq: '9.9.36-53644', napcat: '4.20.0', vcredist: '14.40.33810',
};
let installedAdapter = false;
let epoch = 0;
let scenario: SystemLessonId | undefined;
let settings = structuredClone(baselineSettings);
let savedRevision = 0;
let networkProbes = 0;
let cacheScans = 0;
let cacheResult: CacheCleanupResult | null = null;
const delay = (ms: number) => new Promise<void>(resolve => setTimeout(resolve, ms));
const key = (id: ComponentId, hostId: string) => `${hostId}/${id}`;

function initialCache(): CacheScan {
  return { busy: false, categories: [
    { category: 'installers', files: 2, bytes: 240n * 1024n ** 2n, skipped: 0, warnings: [] },
    { category: 'desktop_updates', files: 1, bytes: 48n * 1024n ** 2n, skipped: 1,
      warnings: ['演示：最近 24 小时的更新包保留。'] },
    { category: 'download_residues', files: 5, bytes: 72n * 1024n ** 2n, skipped: 2, warnings: [] },
  ] };
}
let cache = initialCache();

export function resetSystemDemo(next?: SystemLessonId): void {
  epoch += 1;
  for (const timer of tasks.values()) clearInterval(timer);
  tasks.clear();
  componentActionStore._reset();
  deploymentTaskStore._reset();
  scenario = next;
  installed.clear();
  for (const id of ['qq', 'napcat', 'vcredist'] as const) {
    installed.set(key(id, 'local'), next === 'components' && id !== 'vcredist' ? null : {
      version: versions[id]!, source: '教程演示环境',
    });
  }
  if (next === 'settings') {
    preferencesStore.applySnapshot({ radiusStyle: 'standard' });
    featureTogglesStore.apply({ ...featureTogglesStore.getSnapshot(), apiDebug: false });
  }
  settings = {
    ...structuredClone(baselineSettings),
    uiPreferences: defaultAppUiPreferencesFromPrefs(preferencesStore.get()),
    features: structuredClone(featureTogglesStore.getSnapshot()),
  };
  savedRevision = 0;
  networkProbes = 0;
  cacheScans = 0;
  cacheResult = null;
  cache = initialCache();
}

function detect(id: ComponentId, hostId: string): ComponentDetectResult {
  const original = mockDetect(id, hostId);
  return installed.has(key(id, hostId))
    ? { ...original, detected: installed.get(key(id, hostId)) ?? null }
    : original;
}

function node(id: ComponentId, hostId: string, requiredBy: ComponentId[]): DependencyNode {
  const result = detect(id, hostId);
  return {
    target: { kind: 'component', id }, required_by: requiredBy, version_reqs: [],
    status: !result.supported ? { state: 'unsupported' } : result.detected
      ? { state: 'satisfied', ...result.detected } : { state: 'missing' },
  };
}

function plan(id: ComponentId, hostId: string, phase: RequirementPhase): DependencyPlan {
  const original = mockDependencyPlan(id, hostId, phase);
  return { ...original, nodes: original.nodes.map(value => value.target.kind === 'component'
    ? node(value.target.id, hostId, value.required_by) : value) };
}

function emit(taskId: string, event: ProgressEvent): void {
  emitMockEvent({ kind: 'component_action_progress', task_id: taskId, event });
}

async function runComponentAction(
  id: ComponentId, hostId: string, kind: StepKind, requestedId?: string,
): Promise<string> {
  const taskId = requestedId ?? crypto.randomUUID();
  const runEpoch = epoch;
  const name = mockComponentCatalog.find(value => value.id === id)?.display_name ?? id;
  const message = `${kind === 'uninstall' ? '清理' : '准备'} ${name}（演示）`;
  let percent = 0;
  const now = () => BigInt(Date.now());
  const timer = setInterval(() => {
    if (runEpoch !== epoch) { clearInterval(timer); return; }
    if (percent === 0) {
      emit(taskId, { v: 1, timestamp_ms: now(), kind: 'started', total_steps: 1 });
      emit(taskId, { v: 1, timestamp_ms: now(), kind: 'step_begin', step: 1, message });
    }
    percent += 20;
    emit(taskId, {
      v: 1, timestamp_ms: now(), kind: 'step_progress', step: 1, percent, message,
      speed_bps: 4_800_000n, downloaded_bytes: BigInt(percent) * 280_000n,
      total_bytes: 28_000_000n, download_stage: 'streaming',
    });
    if (percent < 100) return;
    clearInterval(timer);
    tasks.delete(taskId);
    if (kind === 'uninstall') installed.set(key(id, hostId), null);
    else if (kind === 'ensure_installed' || kind === 'force_install' || kind === 'update') {
      installed.set(key(id, hostId), { version: versions[id] ?? '1.0.0', source: '教程演示环境' });
    }
    // The real page refetches as soon as it sees the terminal event.
    emit(taskId, { v: 1, timestamp_ms: now(), kind: 'step_end', step: 1, ok: true });
    emit(taskId, { v: 1, timestamp_ms: now(), kind: 'finished', ok: true });
  }, 380);
  tasks.set(taskId, timer);
  return taskId;
}

export const systemDemoState = {
  get savedRevision() { return savedRevision; },
  get networkProbes() { return networkProbes; },
  get cacheScans() { return cacheScans; },
  get cacheResult() { return cacheResult; },
  get savedSettings() { return structuredClone(settings); },
  isInstalled(id: ComponentId, hostId = 'local') { return detect(id, hostId).detected != null; },
};

export function patchDemoSettings(patch: Partial<BackendSettings>): void {
  settings = { ...settings, ...structuredClone(patch) };
}

export function installSystemDemo(): void {
  if (installedAdapter) return;
  installedAdapter = true;
  resetSystemDemo();
  Object.assign(componentService, {
    listComponents: async () => structuredClone(scenario === 'components'
      ? mockComponentCatalog.filter(item => ['napcat', 'qq', 'vcredist'].includes(item.id))
      : mockComponentCatalog),
    detectComponent: async (id: ComponentId, hostId: string) => structuredClone(detect(id, hostId)),
    resolveDependencies: async (id: ComponentId, hostId: string, phase: RequirementPhase) => plan(id, hostId, phase),
    resolveRuntimeReadiness: async (id: ComponentId, hostId: string) => ({
      root: node(id, hostId, []), plan: plan(id, hostId, 'run'),
    }),
    localQqSource: async () => detect('qq', 'local').detected ? 'managed' : 'missing',
    runComponentAction,
    cancelComponentAction: async (taskId: string) => {
      const timer = tasks.get(taskId);
      if (!timer) return;
      clearInterval(timer);
      tasks.delete(taskId);
      emit(taskId, { v: 1, timestamp_ms: BigInt(Date.now()), kind: 'finished', ok: false });
    },
  });
  Object.assign(settingsService, {
    get: async () => structuredClone(settings),
    set: async (next: BackendSettings) => {
      const error = Object.values(validateNetworkSettings(next.network, next.proxyPassword))[0];
      if (error) throw new Error(error);
      const runEpoch = epoch;
      await delay(300);
      if (runEpoch !== epoch) throw new Error('演示已重置');
      settings = structuredClone(next);
      savedRevision += 1;
    },
  });
  Object.assign(networkService, {
    test: async (network: BackendSettings['network'], password: string) => {
      const error = Object.values(validateNetworkSettings(network, password))[0];
      if (error) throw new Error(error);
      const runEpoch = epoch;
      await delay(850);
      if (runEpoch !== epoch) return [];
      networkProbes += 1;
      return [
        { target: 'GitHub 下载', url: 'https://github.com/NapNeko/NapCatQQ', elapsedMs: 86,
          success: true, message: '演示模拟通过；未发出网络请求' },
        { target: 'Node.js 下载', url: 'https://nodejs.org/dist/', elapsedMs: 112,
          success: true, message: '演示模拟通过；未发出网络请求' },
      ];
    },
  });
  Object.assign(cacheCleanupService, {
    scan: async (): Promise<CacheScan> => {
      const runEpoch = epoch;
      await delay(280);
      if (runEpoch === epoch) cacheScans += 1;
      return structuredClone(cache);
    },
    clean: async (categories: CacheCategory[]): Promise<CacheCleanupResult> => {
      const runEpoch = epoch;
      await delay(700);
      const result: CacheCleanupResult = {
        deleted_files: 0, deleted_bytes: 0n, skipped_files: 0, failed_files: 0, warnings: [],
      };
      if (runEpoch !== epoch) return result;
      const selected = new Set(categories);
      for (const group of cache.categories) {
        if (!selected.has(group.category)) continue;
        result.deleted_files += group.files;
        result.deleted_bytes += group.bytes;
        result.skipped_files += group.skipped;
        result.warnings.push(...group.warnings);
        group.files = 0;
        group.bytes = 0n;
      }
      cacheResult = result;
      return structuredClone(result);
    },
  });
}
