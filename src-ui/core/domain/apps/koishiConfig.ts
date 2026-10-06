// Koishi 类型化配置（koishi.yml 的插件树 + 全局设置）的前端规则：校验和后端 `yml.rs` 同一套，
// 树的增删改都在这里做成纯函数，页面只管拿新树交回去。
//
// 节点在树里的位置用下标路径表示（[2, 0] = 第 3 个节点的第 1 个子节点），和后端校验的错误路径
// `plugins/2/children/0/...` 一一对应。

import type { AppConfigIssue, KoishiInstanceConfig, KoishiPluginNode } from '../../ipc/types';
import type { ConfigFormSpec } from './appConfigForm';

export const KOISHI_DEFAULT_PORT = 5140;
export const KOISHI_LINK_NAME = 'adapter-onebot';
export const KOISHI_LINK_IDENT = 'ncd-link';
export const KOISHI_REVERSE_WS_PATH = '/onebot/ncd';
const GROUP = 'group';

/** 桌面端和控制台靠它们活着：插件页不给删、不给停 */
export const KOISHI_CORE_PLUGINS = new Set(['server', 'console', 'config', 'market', 'logger']);

export type NodePath = readonly number[];

export function isGroup(node: KoishiPluginNode): boolean {
    return node.name === GROUP;
}

export function nodeKey(node: KoishiPluginNode): string {
    return node.ident ? `${node.name}:${node.ident}` : node.name;
}

export function isLinkNode(node: KoishiPluginNode): boolean {
    return node.name === KOISHI_LINK_NAME && node.ident === KOISHI_LINK_IDENT;
}

/** 分组显示名：`$label` 优先，其次标识 */
export function nodeLabel(node: KoishiPluginNode): string {
    const label = node.meta.$label;
    if (typeof label === 'string' && label.trim()) return label;
    if (isGroup(node)) return node.ident || '分组';
    return node.name;
}

export function nodeAt(cfg: KoishiInstanceConfig, path: NodePath): KoishiPluginNode | undefined {
    let list = cfg.plugins;
    let node: KoishiPluginNode | undefined;
    for (const i of path) {
        node = list[i];
        if (!node) return undefined;
        list = node.children;
    }
    return node;
}

/** 换掉某个位置的节点；path 指到不存在的位置就原样返回 */
export function replaceAt(
    cfg: KoishiInstanceConfig,
    path: NodePath,
    fn: (node: KoishiPluginNode) => KoishiPluginNode | null,
): KoishiInstanceConfig {
    const go = (list: KoishiPluginNode[], depth: number): KoishiPluginNode[] => {
        const i = path[depth];
        if (i === undefined || !list[i]) return list;
        const next = list.slice();
        if (depth === path.length - 1) {
            const out = fn(list[i]);
            if (out) next[i] = out;
            else next.splice(i, 1);
            return next;
        }
        next[i] = { ...list[i], children: go(list[i].children, depth + 1) };
        return next;
    };
    return { ...cfg, plugins: go(cfg.plugins, 0) };
}

/** 往某个分组（空路径 = 根）的末尾加节点 */
export function appendTo(
    cfg: KoishiInstanceConfig,
    groupPath: NodePath,
    node: KoishiPluginNode,
): KoishiInstanceConfig {
    if (groupPath.length === 0) return { ...cfg, plugins: [...cfg.plugins, node] };
    return replaceAt(cfg, groupPath, (g) => ({ ...g, children: [...g.children, node] }));
}

/** 分组标识 → 路径（标识全树唯一）；空串 = 根 */
export function groupPathOf(cfg: KoishiInstanceConfig, ident: string): number[] | null {
    if (!ident) return [];
    const go = (list: readonly KoishiPluginNode[], base: number[]): number[] | null => {
        for (let i = 0; i < list.length; i += 1) {
            const n = list[i];
            if (!isGroup(n)) continue;
            if (n.ident === ident) return [...base, i];
            const hit = go(n.children, [...base, i]);
            if (hit) return hit;
        }
        return null;
    };
    return go(cfg.plugins, []);
}

/** 挪到另一个分组（按标识认，空串 = 根）的末尾；不能挪进自己或自己的子分组 */
export function moveTo(
    cfg: KoishiInstanceConfig,
    from: NodePath,
    groupIdent: string,
): KoishiInstanceConfig {
    const node = nodeAt(cfg, from);
    if (!node) return cfg;
    if (
        isGroup(node) &&
        (node.ident === groupIdent ||
            walk(node.children).some((c) => isGroup(c) && c.ident === groupIdent))
    ) {
        return cfg;
    }
    // 先删再按标识重新找目标：删掉后目标分组的下标可能前移
    const removed = replaceAt(cfg, from, () => null);
    const target = groupPathOf(removed, groupIdent);
    if (!target) return cfg;
    return appendTo(removed, target, node);
}

export function walk(nodes: readonly KoishiPluginNode[]): KoishiPluginNode[] {
    return nodes.flatMap((n) => [n, ...walk(n.children)]);
}

/** 真正生效的节点：自己和所有上级分组都开着 */
export function effective(nodes: readonly KoishiPluginNode[]): KoishiPluginNode[] {
    return nodes.filter((n) => n.enabled).flatMap((n) => [n, ...effective(n.children)]);
}

/** 所有分组的标识和显示名，给「移到分组」用；根的标识是空串 */
export function groupChoices(cfg: KoishiInstanceConfig): { ident: string; label: string }[] {
    const out: { ident: string; label: string }[] = [{ ident: '', label: '根' }];
    const go = (list: readonly KoishiPluginNode[], prefix: string) => {
        for (const n of list) {
            if (!isGroup(n) || !n.ident) continue;
            const label = prefix ? `${prefix} / ${nodeLabel(n)}` : nodeLabel(n);
            out.push({ ident: n.ident, label });
            go(n.children, label);
        }
    };
    go(cfg.plugins, '');
    return out;
}

export function usedIdents(cfg: KoishiInstanceConfig): Set<string> {
    return new Set(
        walk(cfg.plugins)
            .map((n) => n.ident)
            .filter(Boolean),
    );
}

/** 上游的 `Math.random().toString(36).slice(2, 8)`：6 位小写字母数字，全树不重复 */
export function freshIdent(cfg: KoishiInstanceConfig, random: () => number = Math.random): string {
    const used = usedIdents(cfg);
    for (;;) {
        let id = '';
        while (id.length < 6) id += Math.floor(random() * 36).toString(36);
        if (!used.has(id)) return id;
    }
}

export function newPlugin(
    cfg: KoishiInstanceConfig,
    name: string,
    enabled = false,
): KoishiPluginNode {
    return { name, ident: freshIdent(cfg), enabled, meta: {}, config: {}, children: [] };
}

export function newGroup(cfg: KoishiInstanceConfig, label: string): KoishiPluginNode {
    const meta: Record<string, unknown> = label.trim() ? { $label: label.trim() } : {};
    return { name: GROUP, ident: freshIdent(cfg), enabled: true, meta, config: {}, children: [] };
}

/** 同一个插件的另一份（多开）：配置照抄，标识另起 */
export function cloneNode(cfg: KoishiInstanceConfig, node: KoishiPluginNode): KoishiPluginNode {
    return { ...structuredClone(node), ident: freshIdent(cfg), enabled: false };
}

function serverNode(cfg: KoishiInstanceConfig): KoishiPluginNode | undefined {
    return (
        effective(cfg.plugins).find((n) => n.name === 'server') ??
        walk(cfg.plugins).find((n) => n.name === 'server')
    );
}

export function koishiServer(cfg: KoishiInstanceConfig): {
    port: number;
    host: string;
    selfUrl: string;
} {
    const c = serverNode(cfg)?.config ?? {};
    return {
        port: typeof c.port === 'number' ? c.port : KOISHI_DEFAULT_PORT,
        host: typeof c.host === 'string' ? c.host : '127.0.0.1',
        selfUrl: typeof c.selfUrl === 'string' ? c.selfUrl : '',
    };
}

/** 改 server 插件的几项；空串 = 删掉这个键（回到上游默认） */
export function setServerField(
    cfg: KoishiInstanceConfig,
    key: 'port' | 'host' | 'selfUrl',
    value: number | string,
): KoishiInstanceConfig {
    const target = serverNode(cfg);
    const setIn = (list: KoishiPluginNode[]): KoishiPluginNode[] =>
        list.map((n) => {
            if (n === target) {
                const config = { ...n.config };
                if (value === '') delete config[key];
                else config[key] = value;
                if (key === 'port') delete config.maxPort;
                return { ...n, config };
            }
            return n.children.length ? { ...n, children: setIn(n.children) } : n;
        });
    return { ...cfg, plugins: setIn(cfg.plugins) };
}

export function linkNode(cfg: KoishiInstanceConfig): KoishiPluginNode | undefined {
    return walk(cfg.plugins).find(isLinkNode);
}

/** 包名 → 插件树里的短名（同上游 loader.keyFor） */
export function koishiShortName(pkg: string): string {
    if (pkg.startsWith('@koishijs/plugin-')) return pkg.slice('@koishijs/plugin-'.length);
    if (pkg.startsWith('koishi-plugin-')) return pkg.slice('koishi-plugin-'.length);
    return pkg.replace('/koishi-plugin-', '/');
}

export function validateKoishiConfig(cfg: KoishiInstanceConfig): AppConfigIssue[] {
    const issues: AppConfigIssue[] = [];
    const port = koishiServer(cfg).port;
    if (!Number.isInteger(port) || port <= 0 || port > 65535) {
        issues.push({ path: 'server/port', message: '端口要在 1 到 65535 之间' });
    }
    const seen = new Set<string>();
    const check = (nodes: readonly KoishiPluginNode[], path: string) => {
        const keys = new Set<string>();
        nodes.forEach((n, i) => {
            const here = `${path}/${i}`;
            const name = n.name.trim();
            if (!name) issues.push({ path: `${here}/name`, message: '插件名不能为空' });
            else if (name.includes(':') || name.startsWith('~') || name.startsWith('$')) {
                issues.push({
                    path: `${here}/name`,
                    message: '插件名不能含冒号，也不能以 ~ / $ 开头',
                });
            }
            if (n.ident.includes(':') || n.ident.includes('~')) {
                issues.push({ path: `${here}/ident`, message: '标识不能含冒号或 ~' });
            }
            if (n.ident) {
                if (seen.has(n.ident))
                    issues.push({ path: `${here}/ident`, message: `标识 ${n.ident} 重复了` });
                seen.add(n.ident);
            }
            const key = nodeKey(n);
            if (keys.has(key))
                issues.push({ path: `${here}/name`, message: `同一分组里 ${key} 出现了两次` });
            keys.add(key);
            if (isGroup(n)) check(n.children, `${here}/children`);
        });
    };
    check(cfg.plugins, 'plugins');
    return issues;
}

/** 错误路径 plugins/2/children/0/name → 节点路径 [2, 0] */
export function nodePathOfIssue(path: string): number[] | null {
    const parts = path.split('/');
    if (parts[0] !== 'plugins') return null;
    const out: number[] = [];
    for (let i = 1; i < parts.length; i += 2) {
        const n = Number(parts[i]);
        if (!Number.isInteger(n)) break;
        out.push(n);
        if (parts[i + 1] !== 'children') break;
    }
    return out.length ? out : null;
}

export const KOISHI_CONFIG_FORM: ConfigFormSpec<'koishi'> = {
    framework: 'koishi',
    validate: validateKoishiConfig,
    saveHint: (r, running) =>
        running
            ? r.port_changed
                ? '已在运行中的 Koishi 里生效，控制台换到了新端口'
                : '已在运行中的 Koishi 里生效'
            : null,
};
