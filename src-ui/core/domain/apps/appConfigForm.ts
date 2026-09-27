// 应用端类型化配置表单里和框架无关的部分：按字段路径收错误、保存成功条的正文、按框架标签取出 / 包回配置。
// 各框架怎么校验、保存后补哪句话，由各自的 *Config.ts 给一份 ConfigFormSpec。

import type { AppConfigIssue, AppConfigWriteResult, AppInstanceConfig } from '../../ipc/types';

// 按标签查表，别用 Extract<…>['data']：泛型下 TS 会把后者拆成各框架的并集，连自己都认不出是同一个类型
type ConfigDataByFramework = { [C in AppInstanceConfig as C['framework']]: C['data'] };
export type AppConfigFramework = keyof ConfigDataByFramework;
export type AppConfigData<F extends AppConfigFramework> = ConfigDataByFramework[F];

export type ConfigFormSpec<F extends AppConfigFramework> = {
    framework: F;
    validate: (config: AppConfigData<F>) => AppConfigIssue[];
    /** 保存成功条最后补的一句（改动怎么生效）；null 就不补 */
    saveHint: (result: AppConfigWriteResult, running: boolean) => string | null;
    /** 写哪份配置档案，只有 AstrBot 分档案 */
    confId?: string;
};

/** 标签是这个框架的才取出来，对不上当没读到 */
export function configDataOf<F extends AppConfigFramework>(
    config: AppInstanceConfig,
    framework: F,
): AppConfigData<F> | null {
    return config.framework === framework ? (config.data as AppConfigData<F>) : null;
}

export function wrapConfigData<F extends AppConfigFramework>(
    framework: F,
    data: AppConfigData<F>,
): AppInstanceConfig {
    return { framework, data } as AppInstanceConfig;
}

/** 同一路径只留第一条，字段下面只挂一句 */
export function issuesByPath(issues: readonly AppConfigIssue[]): Record<string, string> {
    const map: Record<string, string> = {};
    for (const i of issues) if (!(i.path in map)) map[i.path] = i.message;
    return map;
}

/** 保存成功条的正文：端口同步、重新对接这些联动先说，再补框架自己的那句 */
export function configSaveSummary(result: AppConfigWriteResult, hint: string | null): string {
    const parts: string[] = [];
    if (result.port_changed) parts.push('实例端口已同步');
    if (result.relinked) parts.push('已同步更新协议 Bot 侧的对接连接');
    if (hint) parts.push(hint);
    return parts.join('；') || '已保存';
}
