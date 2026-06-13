// runtime_target 在表单与磁盘 JSON 间的语义（与 Rust RuntimeTarget 一致）。

/** 占位：选了「远程」尚未选具体 SSH 档案。 */
export const RUNTIME_TARGET_REMOTE_PLACEHOLDER = 'remote';

/** 磁盘上的 runtime_target 规范为 server profile id（去掉误存的 remote: 前缀）。 */
export function normalizeRuntimeTargetFromDisk(runtimeTarget: string): string {
    if (runtimeTarget.startsWith('remote:')) {
        const rest = runtimeTarget.slice('remote:'.length);
        if (rest.length > 0) return rest;
    }
    return runtimeTarget;
}

export function isRuntimeTargetLocal(runtimeTarget: string): boolean {
    return normalizeRuntimeTargetFromDisk(runtimeTarget) === 'local';
}

/** 已选具体远程主机（server profile id），非 local / remote 占位。 */
export function isRuntimeTargetConcreteRemote(runtimeTarget: string): boolean {
    const t = normalizeRuntimeTargetFromDisk(runtimeTarget);
    return (
        t !== 'local' &&
        t !== RUNTIME_TARGET_REMOTE_PLACEHOLDER
    );
}

/** 运行宿主 Radio：本机 | 远程（含已选具体主机 id 时仍显示远程）。 */
export function runtimeModeForTarget(runtimeTarget: string): 'local' | 'remote' {
    return isRuntimeTargetLocal(runtimeTarget) ? 'local' : 'remote';
}

export function remoteHostIdFromRuntimeTarget(
    runtimeTarget: string,
): string | null {
    const id = normalizeRuntimeTargetFromDisk(runtimeTarget);
    if (!isRuntimeTargetConcreteRemote(id)) return null;
    return `remote:${id}`;
}

/** Select 与 servers 列表对齐用的 profile id。 */
export function serverProfileIdFromRuntimeTarget(runtimeTarget: string): string | null {
    const id = normalizeRuntimeTargetFromDisk(runtimeTarget);
    return isRuntimeTargetConcreteRemote(id) ? id : null;
}