// 浏览器预览里应用端实例的日志：照各框架真实输出的格式（带终端颜色码）拼出来，
// 走查配色、长行换行、续行和重启清空用。

import type { AppInstance } from '../types';
import { emitMockEvent } from './events.mock';

const E = '\x1b[';
const pad = (n: number, w = 2) => String(n).padStart(w, '0');

function clock(d: Date): string {
    return `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function monthDay(d: Date): string {
    return `${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

function millis(d: Date): string {
    return `${clock(d)}.${pad(d.getMilliseconds(), 3)}`;
}

/** 麦麦 lite 样式：时间戳的颜色就是等级，模块名和正文用模块色 */
function mai(d: Date, level: 'info' | 'warn' | 'error', module: string, rgb: string | null, text: string): string {
    const stamp = { info: '38;5;117', warn: '33', error: '31' }[level];
    const head = `${E}${stamp}m${monthDay(d)} ${clock(d)}${E}0m`;
    if (!rgb) return `${head} [${module}] ${text}`;
    return `${head} ${E}38;2;${rgb}m[${module}]${E}0m ${E}38;2;${rgb}m${text}${E}0m`;
}

function astr(d: Date, level: 'INFO' | 'WARN', module: string, where: string, text: string): string {
    const tone = level === 'WARN' ? `${E}33m${E}1m` : `${E}1m`;
    return `${E}32m[${millis(d)}]${E}0m [${module}] ${tone}[${level}]${E}0m [${where}]: ${tone}${text}${E}0m`;
}

function nb(d: Date, level: 'SUCCESS' | 'INFO' | 'WARNING', logger: string, text: string): string {
    const tone = level === 'SUCCESS' ? `${E}32m${E}1m` : level === 'WARNING' ? `${E}33m${E}1m` : `${E}1m`;
    return `${E}32m${monthDay(d)} ${clock(d)}${E}0m [${tone}${level}${E}0m] ${E}36m${E}4m${logger}${E}0m${E}36m${E}0m | ${text}`;
}

function karin(d: Date, level: 'MARK' | 'INFO' | 'WARN', text: string): string {
    const tone = { MARK: '90', INFO: '32', WARN: '33' }[level];
    return `${E}${tone}m[Karin][${millis(d)}][${level}]${E}39m ${text}`;
}

/** Koishi 自己的 logger：非终端时不带颜色，`日期 时间 [级别首字母] 模块 内容` */
function koi(d: Date, level: 'I' | 'W' | 'S', scope: string, text: string): string {
    return `${d.getFullYear()}-${monthDay(d)} ${clock(d)} [${level}] ${scope} ${text}`;
}

function startup(inst: AppInstance, d: Date): string[] {
    const dir = inst.install_dir.replace(/\//g, '\\');
    switch (inst.framework_id) {
        case 'koishi':
            return [
                koi(d, 'I', 'app', 'Koishi/4.18.11'),
                koi(d, 'I', 'loader', 'apply plugin group:server'),
                koi(d, 'I', 'loader', 'apply plugin server:cj4vi7'),
                koi(d, 'I', 'loader', 'apply plugin group:console'),
                koi(d, 'I', 'loader', 'apply plugin console:helk2a'),
                koi(d, 'I', 'loader', 'apply plugin adapter-onebot:ncd-link'),
                koi(d, 'I', 'server', `server listening at http://127.0.0.1:${inst.port}`),
                koi(d, 'I', 'console', `webui is available at http://127.0.0.1:${inst.port}`),
                koi(d, 'I', 'sqlite', 'auto creating table user'),
                koi(d, 'S', 'telemetry', '欢迎使用 Koishi！在您点击「同意」前，telemetry 服务不会启动。'),
            ];
        case 'maibot':
            return [
                mai(d, 'info', '主程序', '255;255;255', '正在启动MaiBot'),
                mai(d, 'info', '日志系统', '128;128;128', '日志系统已初始化：控制台=INFO，文件=DEBUG，轮转=30个文件，清理=30天前'),
                mai(d, 'info', '配置', '162;255;0', 'MaiCore 当前版本: 1.2.5'),
                mai(d, 'warn', '配置', '162;255;0', `配置文件缺失，正在生成默认配置: ${dir}\\config\\model_config.toml`),
                mai(d, 'info', '表情包', '255;175;0', '启动表情包管理器'),
                mai(d, 'info', '记忆存储', '175;135;255', '记事本创建成功！'),
                mai(
                    d,
                    'warn',
                    'NapCat内置适配器',
                    '0;175;135',
                    'NapCat 适配器连接失败: Cannot connect to host 127.0.0.1:29738 ssl:default [远程计算机拒绝网络连接。]；将在 5 秒后重连',
                ),
                mai(d, 'error', '记忆嵌入', null, '编码失败: embedding 任务未配置模型'),
                'Traceback (most recent call last):',
                `  File "${dir}\\src\\chat\\memory\\embedding.py", line 88, in encode`,
                '    raise RuntimeError("embedding 任务未配置模型")',
                'RuntimeError: embedding 任务未配置模型',
            ];
        case 'astrbot':
            return [
                'Welcome to AstrBot CLI!',
                astr(d, 'INFO', 'Core', 'config.astrbot_config:199', 'Config key missing; added default.'),
                astr(
                    d,
                    'INFO',
                    'Core',
                    'utils.event_loop_diagnostics:239',
                    `Event loop watchdog enabled: timeout=30.000s interval=5.000s. If the loop is blocked, Python thread stacks will be written to ${dir}\\data\\logs\\event_loop_watchdog.log`,
                ),
                astr(d, 'INFO', 'Core', 'core.core_lifecycle:364', 'AstrBot started.'),
                `[${d.getFullYear()}-${monthDay(d)} ${clock(d)} +0800] [20504] [INFO] Running on http://0.0.0.0:${inst.port} (CTRL + C to quit)`,
                astr(d, 'WARN', 'astrbot_plugin_vikunja', 'astrbot-plugin-vikunja.main:228', 'Vikunja 插件未配置 URL 或 API Token，提醒调度未启动'),
            ];
        case 'nonebot2':
            return [
                nb(d, 'SUCCESS', 'nonebot', 'NoneBot is initializing...'),
                nb(d, 'INFO', 'nonebot', `Current ${E}33m${E}1mEnv: prod${E}0m${E}33m${E}0m`),
                nb(d, 'WARNING', 'nonebot', 'Legacy project format found! Upgrade with `nb upgrade-format`.'),
                nb(d, 'SUCCESS', 'nonebot', `Succeeded to load plugin "${E}33mnonebot_plugin_translator${E}0m"`),
                nb(d, 'INFO', 'uvicorn', `Uvicorn running on http://127.0.0.1:${inst.port} (Press CTRL+C to quit)`),
            ];
        default:
            return [
                karin(d, 'MARK', 'Karin 启动中...'),
                karin(d, 'INFO', `[server] express 正在监听: http://127.0.0.1:${inst.port}`),
                karin(d, 'WARN', '[plugin] karin-plugin-example 缺少依赖 puppeteer，渲染功能不可用'),
            ];
    }
}

function heartbeat(inst: AppInstance, d: Date, n: number): string {
    switch (inst.framework_id) {
        case 'koishi':
            return koi(d, 'I', 'onebot', `[receive] heartbeat #${n}`);
        case 'maibot':
            return mai(d, 'info', '心流', '255;135;175', `第 ${n} 次观察：群里没有新消息`);
        case 'astrbot':
            return astr(d, 'INFO', 'Core', 'core.event_bus:61', `heartbeat #${n}`);
        case 'nonebot2':
            return nb(d, 'INFO', 'nonebot', `heartbeat #${n}`);
        default:
            return karin(d, 'INFO', `heartbeat #${n} · ws server listening on :${inst.port}`);
    }
}

/** 开页拉历史：一轮完整的启动输出 */
export function mockAppLogTail(inst: AppInstance): string[] {
    return startup(inst, new Date(Date.now() - 10 * 60_000));
}

const heartbeats = new Map<string, ReturnType<typeof setInterval>>();

/**
 * 按后端的顺序放一轮：先 reset，再一串启动输出，之后隔两秒一条心跳，实例停了就收。
 * 麦麦运行卡的重启也走这里，上一轮的心跳先停掉
 */
export function playMockAppRun(inst: AppInstance, isRunning: () => boolean): void {
    clearInterval(heartbeats.get(inst.id));
    emitMockEvent({ kind: 'app_instance_log_reset', instance_id: inst.id });
    startup(inst, new Date()).forEach((line, i) => {
        setTimeout(() => {
            if (isRunning()) emitMockEvent({ kind: 'app_instance_log_appended', instance_id: inst.id, line });
        }, 150 + i * 120);
    });
    let n = 0;
    const timer = setInterval(() => {
        if (!isRunning() || n++ > 20) {
            clearInterval(timer);
            return;
        }
        emitMockEvent({ kind: 'app_instance_log_appended', instance_id: inst.id, line: heartbeat(inst, new Date(), n) });
    }, 2000);
    heartbeats.set(inst.id, timer);
}
