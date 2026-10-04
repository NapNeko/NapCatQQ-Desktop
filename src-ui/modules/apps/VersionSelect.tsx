// 版本选择器：装 / 重装应用端实例时挑版本。
//
// 数据来自 `useAppFrameworkVersions`；后端对不支持的框架返回 null，此时本组件
// 渲染 null，调用方不用自己判断。
//
// 列表由后端按 PEP 440 降序排好（新的在前），这里只负责标出「最新正式版」与
// 「预发布」——预发布默认不会被装（uv 的 if-necessary 策略），必须在界面上说清，
// 否则用户选了一个 pre 却装成 stable 会以为是 bug。

import React from 'react';
import { Select, type SelectItem } from '../../shared/ui';
import { isPrerelease, parsePep440 } from '../../core/domain/apps/appVersions';
import { useAppFrameworkVersions } from '../../hooks/apps/useAppFrameworkVersions';

/** `1.2.1` → `最新 1.2.1`；`1.2.1a1` → `1.2.1a1（预发布）` */
function versionLabel(version: string, latest: string | null): string {
    const parsed = parsePep440(version);
    const pre = parsed ? isPrerelease(parsed) : false;
    const marks: string[] = [];
    if (version === latest) marks.push('最新正式版');
    if (pre) marks.push('预发布');
    return marks.length ? `${version}（${marks.join('、')}）` : version;
}

export interface VersionSelectProps {
    frameworkId: string;
    /** 当前选中的版本；null = 用最新正式版 */
    value: string | null;
    onChange: (version: string | null) => void;
    disabled?: boolean;
    label?: string;
    hint?: React.ReactNode;
}

export const VersionSelect: React.FC<VersionSelectProps> = ({
    frameworkId,
    value,
    onChange,
    disabled = false,
    label = '安装版本',
    hint,
}) => {
    const versions = useAppFrameworkVersions(frameworkId);
    // null = 框架不支持按版本安装：整个控件不出现
    if (versions.data === null) return null;

    const latest = versions.data?.latest ?? null;
    const items: SelectItem[] = [
        {
            value: '__latest__',
            label: latest ? `最新正式版（${latest}）` : '最新正式版',
        },
        ...(versions.data?.versions ?? []).map((v) => ({
            value: v,
            label: versionLabel(v, latest),
        })),
    ];

    const notes: string[] = [];
    if (versions.error) {
        notes.push(`查不到可用版本（${versions.error.message}）；将安装最新正式版`);
    }
    if (versions.data?.has_prerelease) {
        notes.push('该框架有预发布版本；不显式选择时默认装正式版');
    }
    const notesNode =
        notes.length > 0 ? (
            <span className="flex flex-col gap-0.5">
                {notes.map((n) => (
                    <span key={n} className="text-2xs text-text-tertiary">
                        {n}
                    </span>
                ))}
            </span>
        ) : undefined;

    return (
        <Select
            label={label}
            items={items}
            value={value ?? '__latest__'}
            onValueChange={(next) => onChange(next === '__latest__' ? null : next)}
            disabled={disabled || versions.isLoading}
            hint={
                <>
                    {hint}
                    {notesNode}
                </>
            }
        />
    );
};
