// 协议和数据库配置：上游九份默认文档中的 milky / satori / db。
import {
    FormSection,
    NumberField,
    SecretField,
    Select,
    Switch,
    TextField,
} from '../../../../shared/ui';
import { CONFIG_PAIR, ConfigForm } from '../karin/configLayout';
import type { YunzaiTabProps } from './YunzaiBasicTab';

export const YunzaiProtocolsTab: React.FC<YunzaiTabProps> = ({
    config,
    onChange,
    errors,
    disabled,
}) => {
    const { milky, satori, db } = config;
    const setMilky = (patch: Partial<typeof milky>) =>
        onChange({ ...config, milky: { ...milky, ...patch } });
    const setSatori = (patch: Partial<typeof satori>) =>
        onChange({ ...config, satori: { ...satori, ...patch } });
    const setDb = (patch: Partial<typeof db>) =>
        onChange({ ...config, db: { ...db, ...patch } });

    return (
        <ConfigForm>
            <FormSection title="Milky" description="连接 Milky 协议服务，改后重启云崽生效">
                <Switch
                    label="启用 Milky"
                    checked={milky.enable}
                    disabled={disabled}
                    onCheckedChange={(enable) => setMilky({ enable })}
                />
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="服务器地址"
                        hint="可填写主机名或 http / https 地址"
                        value={milky.host}
                        error={errors['milky/host']}
                        disabled={disabled}
                        onValueChange={(host) => setMilky({ host })}
                    />
                    <NumberField
                        label="端口"
                        value={milky.port}
                        min={1}
                        max={65535}
                        error={errors['milky/port']}
                        disabled={disabled}
                        onValueChange={(port) => setMilky({ port: port ?? milky.port })}
                    />
                    <TextField
                        label="URL 前缀"
                        value={milky.prefix}
                        disabled={disabled}
                        onValueChange={(prefix) => setMilky({ prefix })}
                    />
                    <SecretField
                        label="访问 token"
                        value={milky.access_token}
                        disabled={disabled}
                        onValueChange={(access_token) => setMilky({ access_token })}
                    />
                    <Select
                        label="事件接收方式"
                        value={milky.connection}
                        items={[
                            { value: 'ws', label: 'WebSocket' },
                            { value: 'webhook', label: 'Webhook' },
                        ]}
                        error={errors['milky/connection']}
                        disabled={disabled}
                        onValueChange={(connection) => setMilky({ connection })}
                    />
                    <NumberField
                        label="HTTP 请求超时（秒）"
                        hint="0 沿用上游的 15 秒默认值"
                        value={milky.http_timeout}
                        min={0}
                        disabled={disabled}
                        onValueChange={(http_timeout) => setMilky({ http_timeout: http_timeout ?? 15 })}
                    />
                    {milky.connection === 'webhook' ? (
                        <TextField
                            label="Webhook 路径"
                            value={milky.webhook_path}
                            error={errors['milky/webhook_path']}
                            disabled={disabled}
                            onValueChange={(webhook_path) => setMilky({ webhook_path })}
                        />
                    ) : (
                        <>
                            <NumberField
                                label="WebSocket 心跳（秒）"
                                hint="0 沿用上游的 30 秒默认值"
                                value={milky.ws_heartbeat}
                                min={0}
                                disabled={disabled}
                                onValueChange={(ws_heartbeat) => setMilky({ ws_heartbeat: ws_heartbeat ?? 30 })}
                            />
                            <NumberField
                                label="断线重连间隔（秒）"
                                hint="0 沿用上游的 10 秒默认值"
                                value={milky.ws_reconnect_interval}
                                min={0}
                                disabled={disabled}
                                onValueChange={(ws_reconnect_interval) => setMilky({ ws_reconnect_interval: ws_reconnect_interval ?? 10 })}
                            />
                        </>
                    )}
                </div>
            </FormSection>

            <FormSection title="Satori" description="连接 Satori 协议服务，改后重启云崽生效">
                <Switch
                    label="启用 Satori"
                    checked={satori.enable}
                    disabled={disabled}
                    onCheckedChange={(enable) => setSatori({ enable })}
                />
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="HTTP API 地址"
                        hint="留空使用上游默认地址"
                        value={satori.http_endpoint}
                        error={errors['satori/http_endpoint']}
                        disabled={disabled}
                        onValueChange={(http_endpoint) => setSatori({ http_endpoint })}
                    />
                    <TextField
                        label="WebSocket 事件地址"
                        hint="留空使用上游默认地址"
                        value={satori.ws_endpoint}
                        error={errors['satori/ws_endpoint']}
                        disabled={disabled}
                        onValueChange={(ws_endpoint) => setSatori({ ws_endpoint })}
                    />
                    <SecretField
                        label="访问 token"
                        value={satori.token}
                        disabled={disabled}
                        onValueChange={(token) => setSatori({ token })}
                    />
                    <TextField
                        label="平台名称"
                        value={satori.platform}
                        disabled={disabled}
                        onValueChange={(platform) => setSatori({ platform })}
                    />
                    <NumberField
                        label="请求超时（毫秒）"
                        hint="0 沿用上游的 60000 毫秒默认值"
                        value={satori.timeout}
                        min={0}
                        error={errors['satori/timeout']}
                        disabled={disabled}
                        onValueChange={(timeout) => setSatori({ timeout: timeout ?? 60000 })}
                    />
                    <NumberField
                        label="心跳间隔（毫秒）"
                        hint="0 沿用上游的 10000 毫秒默认值"
                        value={satori.heartbeat_interval}
                        min={0}
                        error={errors['satori/heartbeat_interval']}
                        disabled={disabled}
                        onValueChange={(heartbeat_interval) => setSatori({ heartbeat_interval: heartbeat_interval ?? 10000 })}
                    />
                </div>
            </FormSection>

            <FormSection
                title="数据库"
                description="Sequelize 存储设置，改后重启生效；其它方言的连接参数可在原始文件 db.yaml 中设置"
            >
                <div className={CONFIG_PAIR}>
                    <TextField
                        label="数据库方言"
                        hint="例如 sqlite、mysql、postgres、db2、mariadb、mssql"
                        value={db.dialect}
                        disabled={disabled}
                        onValueChange={(dialect) => setDb({ dialect })}
                    />
                    <TextField
                        label="SQLite 文件路径"
                        value={db.storage}
                        disabled={disabled}
                        className="font-mono"
                        onValueChange={(storage) => setDb({ storage })}
                    />
                </div>
                <Switch
                    label="输出数据库日志"
                    checked={db.logging}
                    disabled={disabled}
                    onCheckedChange={(logging) => setDb({ logging })}
                />
            </FormSection>
        </ConfigForm>
    );
};
