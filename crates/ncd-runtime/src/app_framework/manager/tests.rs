use super::link::{bot_listen_ports, remove_link_connection, upsert_link_endpoint};
use super::*;
use ncd_domain::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, ConnectConfig,
    DeploymentType, MessagePostFormat, NetworkBaseFields, WebsocketClientConfig,
    WebsocketServerConfig, WsRole,
};

fn bot() -> BotConfig {
    bot_with(10001, BackendType::NapCat, RuntimeTarget::Local)
}

fn bot_with(qq_id: u64, backend: BackendType, target: RuntimeTarget) -> BotConfig {
    BotConfig {
        bot: BotBasicConfig {
            name: "b".into(),
            qq_id,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: target,
            backend_type: backend,
            deployment_type: DeploymentType::default(),
            snowluma_start_mode: None,
            webui_password_takeover: false,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
        status_command: None,
    }
}

fn ws(name: &str, url: &str) -> WebsocketClientConfig {
    WebsocketClientConfig {
        base: NetworkBaseFields {
            enable: true,
            name: name.into(),
            message_post_format: MessagePostFormat::Array,
            token: "t".into(),
            debug: false,
        },
        url: url.into(),
        report_self_message: false,
        heart_interval: 30000,
        reconnect_interval: 30000,
        role: WsRole::Universal,
    }
}

#[test]
fn upsert_replaces_same_name_and_keeps_others() {
    let mut b = bot();
    b.connect.websocket_clients.push(ws("user", "ws://x"));
    upsert_ws_client(
        &mut b,
        ws("ncd-app:k1", "ws://127.0.0.1:7777/onebot/v11/ws"),
    );
    upsert_ws_client(
        &mut b,
        ws("ncd-app:k1", "ws://127.0.0.1:7801/onebot/v11/ws"),
    );
    assert_eq!(b.connect.websocket_clients.len(), 2);
    assert_eq!(
        b.connect.websocket_clients[1].url,
        "ws://127.0.0.1:7801/onebot/v11/ws"
    );
    assert_eq!(app_link_connections(&b), vec!["ncd-app:k1".to_string()]);
}

fn ws_server(name: &str, port: u16) -> WebsocketServerConfig {
    WebsocketServerConfig {
        base: NetworkBaseFields {
            enable: true,
            name: name.into(),
            message_post_format: MessagePostFormat::Array,
            token: "t".into(),
            debug: false,
        },
        host: "127.0.0.1".into(),
        port,
        report_self_message: false,
        enable_force_push_event: true,
        heart_interval: 30000,
        path: "/".into(),
        role: WsRole::Universal,
    }
}

#[test]
fn link_endpoint_goes_to_the_table_of_its_direction() {
    let mut b = bot();
    b.connect.websocket_servers.push(ws_server("user", 3001));
    upsert_link_endpoint(
        &mut b,
        OneBotLinkEndpoint::WsServer(ws_server("ncd-app:m1", 23001)),
    );
    upsert_link_endpoint(
        &mut b,
        OneBotLinkEndpoint::WsServer(ws_server("ncd-app:m1", 23002)),
    );
    assert!(b.connect.websocket_clients.is_empty());
    assert_eq!(b.connect.websocket_servers.len(), 2);
    assert_eq!(b.connect.websocket_servers[1].port, 23002, "同名替换不追加");
    assert_eq!(app_link_connections(&b), vec!["ncd-app:m1".to_string()]);

    upsert_link_endpoint(
        &mut b,
        OneBotLinkEndpoint::WsClient(ws("ncd-app:k1", "ws://x")),
    );
    assert_eq!(b.connect.websocket_clients.len(), 1);
    assert_eq!(
        app_link_connections(&b),
        vec!["ncd-app:k1".to_string(), "ncd-app:m1".to_string()]
    );
}

#[test]
fn remove_link_connection_only_touches_its_direction() {
    let mut b = bot();
    // 导入认领的是用户自己起名的反向连接；同名的正向服务端是另一回事，不能跟着删
    b.connect
        .websocket_clients
        .push(ws("onebot", "ws://127.0.0.1:8080/onebot/v11/ws"));
    b.connect.websocket_servers.push(ws_server("onebot", 3001));
    assert!(remove_link_connection(
        &mut b,
        "onebot",
        OneBotLinkMode::ReverseWs
    ));
    assert!(b.connect.websocket_clients.is_empty());
    assert_eq!(b.connect.websocket_servers.len(), 1);
    assert!(!remove_link_connection(
        &mut b,
        "ncd-app:m1",
        OneBotLinkMode::ForwardWs
    ));
    assert!(remove_link_connection(
        &mut b,
        "onebot",
        OneBotLinkMode::ForwardWs
    ));
    assert!(b.connect.websocket_servers.is_empty());
}

#[test]
fn bot_listen_ports_cover_every_server_kind() {
    let mut b = bot();
    b.connect.websocket_servers.push(ws_server("a", 3001));
    b.connect.websocket_servers.push(ws_server("zero", 0));
    let ports: Vec<u16> = bot_listen_ports(&b).collect();
    assert_eq!(ports, vec![3001]);
}

#[test]
fn token_and_id_shapes() {
    let t = generate_token();
    assert_eq!(t.len(), 24);
    assert!(t.chars().all(|c| c.is_ascii_alphanumeric()));
    let id = short_id();
    assert_eq!(id.len(), 8);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
}

/// 类型化配置写入的编排分支：版本冲突 / 端口同步 / 已对接时重新 upsert Bot 连接。
/// 用真实本机 Host + 临时目录当 Karin 实例目录（只碰文件，不起进程）。
#[cfg(windows)]
mod write_config {
    use super::*;
    use crate::events::BroadcastEventBus;
    use ncd_appframework::AppFrameworkRegistry;
    use ncd_appframework::AppInstanceConfig;
    use ncd_appframework::karin::config::KarinInstanceConfig;
    use ncd_domain::WebsocketClientConfig;
    use std::sync::Mutex;
    use tokio::sync::Mutex as AsyncMutex;

    struct MemoryBots {
        bots: AsyncMutex<Vec<BotConfig>>,
        upserts: Mutex<usize>,
    }

    #[async_trait::async_trait]
    impl BotConfigPort for MemoryBots {
        async fn bot_config(&self, bot_id: &BotId) -> Result<Option<BotConfig>, String> {
            Ok(self
                .bots
                .lock()
                .await
                .iter()
                .find(|b| b.bot.qq_id.to_string() == bot_id.as_str())
                .cloned())
        }

        async fn upsert_bot_config(&self, config: BotConfig) -> Result<(), String> {
            let mut bots = self.bots.lock().await;
            match bots.iter_mut().find(|b| b.bot.qq_id == config.bot.qq_id) {
                Some(slot) => *slot = config,
                None => bots.push(config),
            }
            *self.upserts.lock().unwrap() += 1;
            Ok(())
        }

        async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
            Ok(self.bots.lock().await.clone())
        }
    }

    struct Fixture {
        _tmp: tempfile::TempDir,
        inst_dir: std::path::PathBuf,
        manager: Arc<AppManager>,
        bots: Arc<MemoryBots>,
        id: AppInstanceId,
    }

    /// 和生产一样接一个组件执行器:插件任务、安装任务都排进它那条队列,测试也从那条队列盯
    fn test_components(root: &Path, bus: &BroadcastEventBus) -> Arc<ComponentExecutor> {
        Arc::new(ComponentExecutor::new(
            crate::components::ComponentExecutorDeps {
                deployment_tasks: DeploymentTaskManager::new(bus.clone()),
                server_manager: Arc::new(ncd_server::ServerManager::new(
                    root,
                    Arc::new(ncd_server::InMemoryCredentialStore::default()),
                )),
                event_bus: bus.clone(),
                app_settings: Arc::new(
                    tokio::sync::RwLock::new(ncd_domain::AppSettings::default()),
                ),
                data_root: root.to_path_buf(),
                local_snowluma_version: None,
                desktop_product_version: "0.0.0".into(),
                registry: Arc::new(AppFrameworkRegistry::with_builtin()),
            },
        ))
    }

    async fn fixture(linked: bool) -> Fixture {
        fixture_on_host(linked, LOCAL_HOST_ID, AppPlacement::LocalNative).await
    }

    async fn fixture_on_host(linked: bool, host_id: &str, placement: AppPlacement) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let inst_dir = tmp.path().join("karin");
        std::fs::create_dir_all(inst_dir.join("@karinjs/config")).unwrap();
        std::fs::write(
                inst_dir.join(".env"),
                "# HTTP监听端口\nHTTP_PORT=7777\n# ws_server鉴权秘钥\nWS_SERVER_AUTH_KEY=tok-1\nLOG_LEVEL=info\n",
            )
            .unwrap();
        std::fs::write(
                inst_dir.join("@karinjs/config/redis.json"),
                "{\"url\":\"redis://127.0.0.1:6379\",\"username\":\"\",\"password\":\"\",\"database\":0}\n",
            )
            .unwrap();

        let bus = Arc::new(BroadcastEventBus::default());
        let store = Arc::new(AppInstanceStore::empty(&root));
        let local: Arc<dyn Host> = Arc::new(ncd_host::local::LocalWindowsHost::new());
        let bots = Arc::new(MemoryBots {
            bots: AsyncMutex::new(vec![
                bot(),
                bot_with(20002, BackendType::SnowLuma, RuntimeTarget::Local),
                bot_with(30003, BackendType::NapCat, RuntimeTarget::server("vps")),
                bot_with(40004, BackendType::SnowLuma, RuntimeTarget::server("vps")),
                bot_with(50005, BackendType::NapCat, RuntimeTarget::server("other")),
            ]),
            upserts: Mutex::new(0),
        });
        let components = test_components(&root, &bus);
        let manager = Arc::new(
            AppManager::new(
                Arc::new(AppFrameworkRegistry::with_builtin()),
                Arc::clone(&store),
                Arc::new(NativeAppRuntime::new(Arc::clone(&bus), Arc::clone(&store))),
                Arc::new(ncd_server::LocalOnlyHostResolver::new(local)),
                bots.clone(),
                bus,
                &root,
            )
            .with_component_executor(components),
        );
        let id = AppInstanceId::new("k1");
        let install_dir = HostPath::from_windows(inst_dir.to_string_lossy().as_ref());
        store
            .upsert(AppInstance {
                id: id.clone(),
                framework_id: AppFrameworkId::new("karin"),
                display_name: "Karin".into(),
                placement,
                host_id: host_id.to_string(),
                install_dir: install_dir.as_posix().to_string(),
                port: 7777,
                state: AppInstanceState::Stopped,
                link: linked.then(|| AppLinkRecord {
                    bot_id: BotId::new("10001"),
                    mode: OneBotLinkMode::ReverseWs,
                    connection_name: app_link_connection_name(&id),
                    linked_at_ms: 1,
                    resident_forward_port: None,
                }),
                installed_version: Some("1.0.0".into()),
                last_error: None,
                created_at_ms: 1,
                install_renderer: true,
                origin: ncd_domain::AppInstanceOrigin::Created,
                auto_start: true,
            })
            .await
            .unwrap();
        Fixture {
            _tmp: tmp,
            inst_dir,
            manager,
            bots,
            id,
        }
    }

    /// MaiBot 实例目录：已装好的样子（源码只放对接 / 配置 / 条款用得上的几个文件）
    /// 每个测试给不同的实例 id：正向听口按「实例:QQ」散列起点再探 bind，
    /// 并行的测试若共用一个 id 会同时探同一个口，互相把对方挤到下一个口
    async fn maibot_fixture(instance_id: &str) -> Fixture {
        let local: Arc<dyn Host> = Arc::new(ncd_host::local::LocalWindowsHost::new());
        maibot_fixture_with(
            instance_id,
            LOCAL_HOST_ID,
            Arc::new(ncd_server::LocalOnlyHostResolver::new(local)),
        )
        .await
    }

    /// `host_id` 是麦麦装在哪；文件总落在本机临时目录，远端由 `resolver` 给的替身主机转手
    async fn maibot_fixture_with(
        instance_id: &str,
        host_id: &str,
        resolver: Arc<dyn HostResolver>,
    ) -> Fixture {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        let inst_dir = tmp.path().join("maibot");
        for dir in [
            "config",
            "data",
            "src/config",
            "plugins/MaiBot-Napcat-Adapter",
        ] {
            std::fs::create_dir_all(inst_dir.join(dir)).unwrap();
        }
        std::fs::write(
                inst_dir.join("config/bot_config.toml"),
                "[inner]\nversion = \"8.14.40\"\n\n[webui]\nport = 23001\n\n[maim_message]\nws_server_port = 23002\n",
            )
            .unwrap();
        std::fs::write(
            inst_dir.join("data/webui.json"),
            "{\"access_token\":\"Ncd_tok\",\"token_source\":\"configured\"}\n",
        )
        .unwrap();
        std::fs::write(
            inst_dir.join("src/config/config.py"),
            "CONFIG_VERSION: str = \"8.14.40\"\n",
        )
        .unwrap();
        std::fs::write(
            inst_dir.join("plugins/MaiBot-Napcat-Adapter/_manifest.json"),
            "{\"host_application\":{\"min_version\":\"1.2.0\",\"max_version\":\"1.2.99\"}}",
        )
        .unwrap();
        std::fs::write(inst_dir.join("EULA.md"), "# EULA\r\n条款一\r\n").unwrap();
        std::fs::write(inst_dir.join("PRIVACY.md"), "# PRIVACY\n条款二\n").unwrap();

        let bus = Arc::new(BroadcastEventBus::default());
        let store = Arc::new(AppInstanceStore::empty(&root));
        let bots = Arc::new(MemoryBots {
            bots: AsyncMutex::new(vec![
                bot(),
                bot_with(20002, BackendType::SnowLuma, RuntimeTarget::Local),
                bot_with(30003, BackendType::NapCat, RuntimeTarget::server("vps")),
                bot_with(50005, BackendType::NapCat, RuntimeTarget::server("vps2")),
            ]),
            upserts: Mutex::new(0),
        });
        let components = test_components(&root, &bus);
        let manager = Arc::new(
            AppManager::new(
                Arc::new(AppFrameworkRegistry::with_builtin()),
                Arc::clone(&store),
                Arc::new(NativeAppRuntime::new(Arc::clone(&bus), Arc::clone(&store))),
                resolver,
                bots.clone(),
                bus,
                &root,
            )
            .with_component_executor(components),
        );
        let id = AppInstanceId::new(instance_id);
        let install_dir = HostPath::from_windows(inst_dir.to_string_lossy().as_ref());
        store
            .upsert(AppInstance {
                id: id.clone(),
                framework_id: AppFrameworkId::new("maibot"),
                display_name: "麦麦".into(),
                placement: AppPlacement::native_for_host(host_id),
                host_id: host_id.to_string(),
                install_dir: install_dir.as_posix().to_string(),
                port: 23001,
                state: AppInstanceState::Stopped,
                link: None,
                installed_version: Some("1.2.5".into()),
                last_error: None,
                created_at_ms: 1,
                install_renderer: false,
                origin: ncd_domain::AppInstanceOrigin::Created,
                auto_start: false,
            })
            .await
            .unwrap();
        Fixture {
            _tmp: tmp,
            inst_dir,
            manager,
            bots,
            id,
        }
    }

    fn adapter_text(f: &Fixture) -> String {
        std::fs::read_to_string(f.inst_dir.join("plugins/MaiBot-Napcat-Adapter/config.toml"))
            .unwrap()
    }

    fn adapter_cfg(f: &Fixture) -> ncd_appframework::MaiBotAdapterConfig {
        ncd_appframework::maibot::config::read_adapter_config(Some(&adapter_text(f)))
    }

    #[tokio::test]
    async fn maibot_forward_link_opens_bot_server_and_points_adapter_at_it() {
        let f = maibot_fixture("m1").await;
        let bot_id = BotId::new("10001");
        let plan = f.manager.preview_link(&f.id, &bot_id).await.unwrap();
        let server = plan
            .connection
            .as_ws_server()
            .expect("MaiBot 是正向对接")
            .clone();
        assert!(server.port >= 20_000, "听口由编排层分配：{}", server.port);
        let again = f.manager.preview_link(&f.id, &bot_id).await.unwrap();
        assert_eq!(
            again.connection.as_ws_server().unwrap().port,
            server.port,
            "预览和写入要同一个口"
        );

        let linked = f.manager.apply_link(&f.id, &bot_id).await.unwrap();
        let link = linked.link.expect("已对接");
        assert_eq!(link.mode, OneBotLinkMode::ForwardWs);
        assert_eq!(link.connection_name, "ncd-app:m1");

        let bot = f.bots.bot_config(&bot_id).await.unwrap().unwrap();
        assert!(bot.connect.websocket_clients.is_empty(), "正向不写客户端表");
        let s = &bot.connect.websocket_servers[0];
        assert_eq!(
            (s.base.name.as_str(), s.host.as_str(), s.port),
            ("ncd-app:m1", "127.0.0.1", server.port)
        );

        let a = adapter_cfg(&f);
        assert!(a.enabled);
        assert_eq!(a.napcat_port, server.port);
        assert!(adapter_text(&f).contains("config_version = \"0.1.0\""));
        assert_eq!(
            ncd_appframework::maibot::config::read_adapter_token(Some(&adapter_text(&f)))
                .as_deref(),
            Some(s.base.token.as_str())
        );

        // 重新对接沿用原口和 token
        f.manager.apply_link(&f.id, &bot_id).await.unwrap();
        let bot = f.bots.bot_config(&bot_id).await.unwrap().unwrap();
        assert_eq!(bot.connect.websocket_servers.len(), 1);
        assert_eq!(bot.connect.websocket_servers[0].port, server.port);
        assert_eq!(bot.connect.websocket_servers[0].base.token, s.base.token);

        let unlinked = f.manager.unlink(&f.id).await.unwrap();
        assert!(unlinked.link.is_none());
        let bot = f.bots.bot_config(&bot_id).await.unwrap().unwrap();
        assert!(bot.connect.websocket_servers.is_empty());
        assert!(
            !adapter_cfg(&f).enabled,
            "解绑关掉适配器，免得对着删掉的服务端重连刷日志"
        );
    }

    #[tokio::test]
    async fn maibot_forward_port_avoids_other_bot_servers_on_the_same_host() {
        let f = maibot_fixture("m2").await;
        let first = f
            .manager
            .preview_link(&f.id, &BotId::new("10001"))
            .await
            .unwrap()
            .connection
            .as_ws_server()
            .unwrap()
            .port;
        // 同机另一台 Bot 已经在这个口上开了服务端
        let mut other = f
            .bots
            .bot_config(&BotId::new("20002"))
            .await
            .unwrap()
            .unwrap();
        other.connect.websocket_servers.push(WebsocketServerConfig {
            base: ncd_domain::NetworkBaseFields {
                enable: true,
                name: "user".into(),
                message_post_format: ncd_domain::MessagePostFormat::Array,
                token: String::new(),
                debug: false,
            },
            host: "0.0.0.0".into(),
            port: first,
            report_self_message: false,
            enable_force_push_event: true,
            heart_interval: 30000,
            path: "/".into(),
            role: ncd_domain::WsRole::Universal,
        });
        f.bots.upsert_bot_config(other).await.unwrap();
        let second = f
            .manager
            .preview_link(&f.id, &BotId::new("10001"))
            .await
            .unwrap()
            .connection
            .as_ws_server()
            .unwrap()
            .port;
        assert_ne!(second, first);
    }

    /// 远端替身：文件照样落在本机临时目录（实例目录在那），但自称远端；开隧道只记下请求、
    /// 按请求的口（没指定就给固定口）回句柄；跑命令一律回空，占口探测、时区都当没有
    struct TunnelHost {
        fs: ncd_host::local::LocalWindowsHost,
        specs: Mutex<Vec<TunnelSpec>>,
    }

    const AUTO_LOCAL: u16 = 41001;
    const AUTO_REMOTE: u16 = 42002;

    impl TunnelHost {
        fn new() -> Arc<Self> {
            Arc::new(Self {
                fs: ncd_host::local::LocalWindowsHost::new(),
                specs: Mutex::new(Vec::new()),
            })
        }

        fn specs(&self) -> Vec<TunnelSpec> {
            self.specs.lock().unwrap().clone()
        }
    }

    #[async_trait::async_trait]
    impl Host for TunnelHost {
        fn os(&self) -> Os {
            self.fs.os()
        }
        fn arch(&self) -> ncd_host::Arch {
            self.fs.arch()
        }
        fn locality(&self) -> Locality {
            Locality::Remote
        }
        fn id(&self) -> &str {
            "remote:test"
        }
        fn shell(&self) -> &dyn ncd_host::HostShell {
            self.fs.shell()
        }
        async fn read_file(&self, p: &HostPath) -> Result<bytes::Bytes, ncd_host::HostError> {
            self.fs.read_file(p).await
        }
        async fn write_file(&self, p: &HostPath, b: &[u8]) -> Result<(), ncd_host::HostError> {
            self.fs.write_file(p, b).await
        }
        async fn list_dir(
            &self,
            p: &HostPath,
        ) -> Result<Vec<ncd_host::DirEntry>, ncd_host::HostError> {
            self.fs.list_dir(p).await
        }
        async fn create_dir_all(&self, p: &HostPath) -> Result<(), ncd_host::HostError> {
            self.fs.create_dir_all(p).await
        }
        async fn remove_file(&self, p: &HostPath) -> Result<(), ncd_host::HostError> {
            self.fs.remove_file(p).await
        }
        async fn remove_dir_all(&self, p: &HostPath) -> Result<(), ncd_host::HostError> {
            self.fs.remove_dir_all(p).await
        }
        async fn rename(&self, a: &HostPath, b: &HostPath) -> Result<(), ncd_host::HostError> {
            self.fs.rename(a, b).await
        }
        async fn exists(&self, p: &HostPath) -> Result<bool, ncd_host::HostError> {
            self.fs.exists(p).await
        }
        async fn upload(
            &self,
            l: &std::path::Path,
            r: &HostPath,
        ) -> Result<(), ncd_host::HostError> {
            self.fs.upload(l, r).await
        }
        async fn download(
            &self,
            r: &HostPath,
            l: &std::path::Path,
        ) -> Result<(), ncd_host::HostError> {
            self.fs.download(r, l).await
        }
        async fn extract_archive(
            &self,
            a: &HostPath,
            d: &HostPath,
            k: ncd_host::ArchiveKind,
        ) -> Result<(), ncd_host::HostError> {
            self.fs.extract_archive(a, d, k).await
        }
        async fn spawn(
            &self,
            _: HostCommand,
        ) -> Result<Box<dyn ncd_host::HostProcess>, ncd_host::HostError> {
            Err(ncd_host::HostError::Unsupported { operation: "spawn" })
        }
        async fn run_to_string(
            &self,
            _: HostCommand,
        ) -> Result<ncd_host::CommandOutput, ncd_host::HostError> {
            Ok(ncd_host::CommandOutput {
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
            })
        }
        async fn open_tunnel(&self, spec: TunnelSpec) -> Result<TunnelHandle, ncd_host::HostError> {
            self.specs.lock().unwrap().push(spec.clone());
            Ok(match spec.direction {
                ncd_host::remote::TunnelDirection::LocalToRemote => {
                    let local = if spec.local_port == 0 {
                        AUTO_LOCAL
                    } else {
                        spec.local_port
                    };
                    TunnelHandle::detached(local, 0)
                }
                ncd_host::remote::TunnelDirection::RemoteToLocal => {
                    let remote = if spec.remote_port == 0 {
                        AUTO_REMOTE
                    } else {
                        spec.remote_port
                    };
                    TunnelHandle::detached(spec.local_port, remote)
                }
            })
        }
    }

    struct VpsResolver {
        local: Arc<dyn Host>,
        vps: Arc<TunnelHost>,
        vps2: Arc<TunnelHost>,
    }

    #[async_trait::async_trait]
    impl HostResolver for VpsResolver {
        async fn resolve(
            &self,
            target: &RuntimeTarget,
        ) -> Result<Arc<dyn Host>, ncd_server::HostResolveError> {
            match target.server_id() {
                None => Ok(Arc::clone(&self.local)),
                Some("vps") => Ok(self.vps.clone() as Arc<dyn Host>),
                Some("vps2") => Ok(self.vps2.clone() as Arc<dyn Host>),
                Some(other) => Err(ncd_server::HostResolveError::message(format!(
                    "没有 {other}"
                ))),
            }
        }
    }

    async fn tunnel_fixture(
        instance_id: &str,
        host_id: &str,
    ) -> (Fixture, Arc<TunnelHost>, Arc<TunnelHost>) {
        let vps = TunnelHost::new();
        let vps2 = TunnelHost::new();
        let resolver = Arc::new(VpsResolver {
            local: Arc::new(ncd_host::local::LocalWindowsHost::new()),
            vps: vps.clone(),
            vps2: vps2.clone(),
        });
        (
            maibot_fixture_with(instance_id, host_id, resolver).await,
            vps,
            vps2,
        )
    }

    /// 远端 Bot + 本机麦麦：Bot 在自己机上听 P，桌面端 `-L` 把 P 接到本机 Q，适配器连 Q；
    /// 桌面端重开时隧道先要适配器里那个口，要到了就不改适配器
    #[tokio::test]
    async fn maibot_reaches_a_remote_bot_through_a_desktop_l_tunnel() {
        let (f, vps, _) = tunnel_fixture("m3", LOCAL_HOST_ID).await;
        let bot_id = BotId::new("30003");
        let linked = f.manager.apply_link(&f.id, &bot_id).await.unwrap();
        let link = linked.link.clone().expect("已对接");
        assert_eq!(link.mode, OneBotLinkMode::ForwardWs);
        assert_eq!(link.resident_forward_port, None, "桌面端握着的隧道不算常驻");

        let bot = f.bots.bot_config(&bot_id).await.unwrap().unwrap();
        let s = bot.connect.websocket_servers[0].clone();
        assert_eq!(
            (s.base.name.as_str(), s.host.as_str()),
            ("ncd-app:m3", "127.0.0.1")
        );
        let a = adapter_cfg(&f);
        assert!(a.enabled);
        assert_eq!(
            (a.napcat_host.as_str(), a.napcat_port),
            ("127.0.0.1", AUTO_LOCAL),
            "适配器连的是隧道口，不是 Bot 的口"
        );
        let specs = vps.specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(
            specs[0].direction,
            ncd_host::remote::TunnelDirection::LocalToRemote
        );
        assert_eq!((specs[0].local_port, specs[0].remote_port), (0, s.port));

        f.manager.apply_link(&f.id, &bot_id).await.unwrap();
        assert_eq!(vps.specs().len(), 1, "隧道还在就不重开");

        f.manager.drop_instance_tunnel(&f.id).await;
        f.manager.reconcile_link_tunnel(&linked).await.unwrap();
        let specs = vps.specs();
        assert_eq!(specs.len(), 2);
        assert_eq!(
            specs[1].local_port, AUTO_LOCAL,
            "重开时先要适配器里写着的口"
        );
        assert_eq!(adapter_cfg(&f).napcat_port, AUTO_LOCAL);

        let unlinked = f.manager.unlink(&f.id).await.unwrap();
        assert!(unlinked.link.is_none());
        assert!(
            f.bots
                .bot_config(&bot_id)
                .await
                .unwrap()
                .unwrap()
                .connect
                .websocket_servers
                .is_empty()
        );
        assert!(!adapter_cfg(&f).enabled);
        assert!(
            !f.manager
                .tunnels
                .lock()
                .await
                .contains_key(&forward_tunnel_key(&f.id))
        );
    }

    /// 本机 Bot + 远端麦麦：桌面端对麦麦那台机开 `-R`，在那台机的回环上听 Q、接回本机的 P
    #[tokio::test]
    async fn maibot_on_a_remote_host_reaches_a_local_bot_through_a_desktop_r_tunnel() {
        let (f, vps, _) = tunnel_fixture("m3r", "remote:vps").await;
        let bot_id = BotId::new("10001");
        f.manager.apply_link(&f.id, &bot_id).await.unwrap();
        let bot = f.bots.bot_config(&bot_id).await.unwrap().unwrap();
        let p = bot.connect.websocket_servers[0].port;
        let specs = vps.specs();
        assert_eq!(specs.len(), 1);
        assert_eq!(
            specs[0].direction,
            ncd_host::remote::TunnelDirection::RemoteToLocal
        );
        assert_eq!((specs[0].remote_port, specs[0].local_port), (0, p));
        assert_eq!(adapter_cfg(&f).napcat_port, AUTO_REMOTE);
    }

    /// 两台远端：要应用机能 SSH 上 Bot 机；替身拿不到拨号地址（也不是 Linux），一步都不该落
    #[tokio::test]
    async fn maibot_between_two_remotes_needs_the_resident_tunnel_before_touching_anything() {
        let (f, vps, vps2) = tunnel_fixture("m3rr", "remote:vps").await;
        let err = f
            .manager
            .apply_link(&f.id, &BotId::new("50005"))
            .await
            .unwrap_err();
        assert!(matches!(err, AppFrameworkError::Validation(_)), "{err}");
        assert!(
            vps.specs().is_empty() && vps2.specs().is_empty(),
            "不走桌面端隧道"
        );
        assert!(
            !f.inst_dir
                .join("plugins/MaiBot-Napcat-Adapter/config.toml")
                .exists()
        );
        assert_eq!(*f.bots.upserts.lock().unwrap(), 0);
    }

    /// Docker 部署的 Bot 在容器里：它开的服务、连的 127.0.0.1 都是容器自己的，同机也连不上
    #[tokio::test]
    async fn docker_bot_is_refused_before_touching_anything() {
        let f = maibot_fixture("m-docker").await;
        let bot_id = BotId::new("10001");
        let mut bot = f.bots.bot_config(&bot_id).await.unwrap().unwrap();
        bot.bot.deployment_type = DeploymentType::Docker;
        f.bots.upsert_bot_config(bot).await.unwrap();
        let upserts = *f.bots.upserts.lock().unwrap();

        for err in [
            f.manager.preview_link(&f.id, &bot_id).await.unwrap_err(),
            f.manager.apply_link(&f.id, &bot_id).await.unwrap_err(),
        ] {
            assert!(
                matches!(err, AppFrameworkError::LinkModeUnsupported(_)),
                "{err}"
            );
            assert!(err.to_string().contains("Docker"), "{err}");
        }
        assert!(
            !f.inst_dir
                .join("plugins/MaiBot-Napcat-Adapter/config.toml")
                .exists()
        );
        assert_eq!(*f.bots.upserts.lock().unwrap(), upserts, "Bot 侧一条没写");
    }

    #[tokio::test]
    async fn maibot_terms_pending_until_accepted_and_again_after_change() {
        let f = maibot_fixture("m4").await;
        let pending = f.manager.pending_terms(&f.id).await.unwrap();
        let ids: Vec<&str> = pending.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, vec!["eula", "privacy"]);
        assert!(pending[0].text.contains("条款一"));
        let err = f.manager.start_instance(&f.id).await.unwrap_err();
        assert!(err.to_string().contains("同意"), "{err}");
        let inst = f.manager.get_instance(&f.id).await.unwrap();
        assert!(
            inst.last_error
                .as_deref()
                .is_some_and(|e| e.contains("同意")),
            "起不来的原因要落盘"
        );

        f.manager.accept_terms(&f.id).await.unwrap();
        assert!(f.manager.pending_terms(&f.id).await.unwrap().is_empty());
        let confirmed = std::fs::read_to_string(f.inst_dir.join("eula.confirmed")).unwrap();
        assert_eq!(
            confirmed.len(),
            32,
            "上游原样比对，不能带换行：{confirmed:?}"
        );

        std::fs::write(f.inst_dir.join("PRIVACY.md"), "# PRIVACY\n条款二（修订）\n").unwrap();
        let pending = f.manager.pending_terms(&f.id).await.unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, "privacy");
    }

    /// uv 先建 venv 再装包：目录到这一步 detect 就认「装好了」，其实包还没装完
    fn make_dir_look_installed(f: &Fixture) {
        std::fs::write(f.inst_dir.join("bot.py"), "").unwrap();
        std::fs::write(
            f.inst_dir.join("pyproject.toml"),
            "[project]\nname = \"MaiBot\"\nversion = \"1.2.5\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(f.inst_dir.join(".venv/Scripts")).unwrap();
        std::fs::write(f.inst_dir.join(".venv/Scripts/python.exe"), "").unwrap();
    }

    /// 假的安装任务：跑到收到信号为止，true 成功、false 失败
    async fn gated_install_task(
        tasks: &DeploymentTaskManager,
        task_id: &str,
    ) -> tokio::sync::oneshot::Sender<bool> {
        let (release, gate) = tokio::sync::oneshot::channel::<bool>();
        tasks
            .submit(crate::deploy::DeploymentTaskRequest {
                task_id: task_id.into(),
                kind: ncd_domain::DeploymentTaskKind::ComponentAction {
                    component_id: "maibot".into(),
                    action: "ensure_installed".into(),
                },
                host_id: LOCAL_HOST_ID.into(),
                title: "maibot ensure_installed".into(),
                resources: vec![],
                depends_on: vec![],
                dedupe_key: None,
                cancellable: false,
                runner: Box::new(move |_| {
                    Box::pin(async move {
                        if gate.await.unwrap_or(false) {
                            crate::deploy::DeploymentTaskRunResult::ok("ok")
                        } else {
                            crate::deploy::DeploymentTaskRunResult::failed("uv sync 失败")
                        }
                    })
                }),
            })
            .await;
        release
    }

    async fn wait_state(f: &Fixture, want: AppInstanceState) -> AppInstance {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let inst = f.manager.get_instance(&f.id).await.unwrap();
            if inst.state == want && !f.manager.install_watched(&f.id) {
                return inst;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "等不到 {want:?}，现在 {:?}，还在盯：{}",
                inst.state,
                f.manager.install_watched(&f.id)
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    #[tokio::test]
    async fn install_state_follows_the_task_not_the_half_built_dir() {
        let f = maibot_fixture("m-inst-ok").await;
        make_dir_look_installed(&f);
        let tasks = f.manager.task_queue().unwrap().clone();
        let release = gated_install_task(&tasks, "t-ok").await;

        let inst = f.manager.track_install(&f.id, "t-ok".into()).await.unwrap();
        assert_eq!(inst.state, AppInstanceState::Installing);
        // 过一轮兜底轮询，再手动刷新一次：任务没完就一直是安装中
        tokio::time::sleep(INSTALL_POLL_INTERVAL + Duration::from_millis(500)).await;
        let inst = f.manager.refresh_instance(&f.id).await.unwrap();
        assert_eq!(inst.state, AppInstanceState::Installing);

        release.send(true).unwrap();
        let inst = wait_state(&f, AppInstanceState::Installed).await;
        assert_eq!(inst.installed_version.as_deref(), Some("1.2.5"));
        assert_eq!(inst.last_error, None);
    }

    #[tokio::test]
    async fn install_failure_lands_as_not_installed_with_the_task_error() {
        let f = maibot_fixture("m-inst-fail").await;
        let tasks = f.manager.task_queue().unwrap().clone();
        let release = gated_install_task(&tasks, "t-fail").await;
        f.manager
            .track_install(&f.id, "t-fail".into())
            .await
            .unwrap();

        release.send(false).unwrap();
        let inst = wait_state(&f, AppInstanceState::NotInstalled).await;
        assert!(
            inst.last_error
                .as_deref()
                .is_some_and(|e| e.contains("uv sync 失败")),
            "{:?}",
            inst.last_error
        );
    }

    #[tokio::test]
    async fn unwatched_installing_settles_from_the_dir() {
        // 上次装到一半桌面端退了：没人盯的「安装中」在刷新 / 冷启动对账时按目录收尾
        let f = maibot_fixture("m-inst-orphan").await;
        f.manager
            .store
            .update(&f.id, |i| i.state = AppInstanceState::Installing)
            .await
            .unwrap();
        let inst = f.manager.refresh_instance(&f.id).await.unwrap();
        assert_eq!(inst.state, AppInstanceState::NotInstalled);
        assert!(
            inst.last_error
                .as_deref()
                .is_some_and(|e| e.contains("没有完成"))
        );

        // 任务在队列里查不到（结束后被清掉了）：同样按目录收尾，目录是好的就算装好
        make_dir_look_installed(&f);
        f.manager
            .track_install(&f.id, "t-gone".into())
            .await
            .unwrap();
        let inst = wait_state(&f, AppInstanceState::Installed).await;
        assert_eq!(inst.installed_version.as_deref(), Some("1.2.5"));
    }

    #[tokio::test]
    async fn maibot_config_writes_ports_and_chat_filter() {
        let f = maibot_fixture("m5").await;
        let env = f.manager.read_config(&f.id).await.unwrap();
        let AppInstanceConfig::MaiBot(mut cfg) = env.config.clone() else {
            panic!("expected MaiBot config");
        };
        assert_eq!((cfg.webui_port(), cfg.legacy_ws_port()), (23001, 23002));
        assert_eq!(cfg.webui_token, "Ncd_tok");
        let adapter = cfg.adapter.as_mut().expect("适配器目录在");
        assert!(adapter.chat.drops_everything());
        adapter.chat.group_list.push("123456".into());
        cfg.bot.webui.port = 23011;

        let res = f
            .manager
            .write_config(&f.id, AppInstanceConfig::MaiBot(cfg), Some(env.revision))
            .await
            .unwrap();
        assert!(res.port_changed);
        assert!(!res.relinked);
        assert_eq!(f.manager.get_instance(&f.id).await.unwrap().port, 23011);
        let AppInstanceConfig::MaiBot(after) = res.config else {
            panic!("expected MaiBot config");
        };
        assert_eq!(after.adapter.unwrap().chat.group_list, vec!["123456"]);
        let bot_cfg = std::fs::read_to_string(f.inst_dir.join("config/bot_config.toml")).unwrap();
        assert!(bot_cfg.contains("port = 23011"), "{bot_cfg}");
        assert!(bot_cfg.contains("version = \"8.14.40\""));
    }

    #[tokio::test]
    async fn maibot_stopped_write_touches_only_changed_keys() {
        let f = maibot_fixture("m-typed").await;
        std::fs::write(
            f.inst_dir.join("src/config/config.py"),
            "CONFIG_VERSION: str = \"8.14.40\"\nMODEL_CONFIG_VERSION: str = \"1.17.9\"\n",
        )
        .unwrap();

        let env = f.manager.read_config(&f.id).await.unwrap();
        let AppInstanceConfig::MaiBot(mut cfg) = env.config.clone() else {
            panic!("expected MaiBot config");
        };
        cfg.bot.personality.personality = "新人格".into();
        cfg.models.api_providers[0].api_key = "sk-test".into();
        let res = f
            .manager
            .write_config(&f.id, AppInstanceConfig::MaiBot(cfg), Some(env.revision))
            .await
            .unwrap();
        assert!(!res.restart_required);
        let bot_cfg = std::fs::read_to_string(f.inst_dir.join("config/bot_config.toml")).unwrap();
        assert!(
            bot_cfg.starts_with("[inner]\nversion = \"8.14.40\"\n\n[webui]\nport = 23001\n"),
            "{bot_cfg}"
        );
        assert!(
            bot_cfg.contains("[personality]\npersonality = \"新人格\""),
            "{bot_cfg}"
        );
        assert!(
            !bot_cfg.contains("[chat"),
            "没改的默认值不写出来：{bot_cfg}"
        );
        let model_cfg =
            std::fs::read_to_string(f.inst_dir.join("config/model_config.toml")).unwrap();
        assert!(
            model_cfg.starts_with("[inner]\nversion = \"1.17.9\""),
            "版本号取实例源码里的常量"
        );
        let models =
            ncd_appframework::maibot::schema::read_model_config_file(Some(&model_cfg)).unwrap();
        assert_eq!(models.api_providers[0].api_key, "sk-test");
        assert!(
            model_cfg.contains("# 模型标识符"),
            "从上游默认文件起的稿，注释带着"
        );
    }

    /// 运行中不碰文件：bot 只把改动交给麦麦 WebUI 合并，model 整份交；按字段提示重启；两次写留间隔
    #[tokio::test]
    async fn maibot_running_write_goes_through_webui() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let f = maibot_fixture("m-live").await;
        let server = MockServer::start().await;
        let port = server.address().port();
        let bot_path = f.inst_dir.join("config/bot_config.toml");
        let seed = format!(
            "[inner]\nversion = \"8.14.40\"\n\n[webui]\nport = {port}\n\n[maim_message]\nws_server_port = 23002\n"
        );
        std::fs::write(&bot_path, &seed).unwrap();
        f.manager
            .store
            .update(&f.id, |i| {
                i.port = port;
                i.state = AppInstanceState::Running;
            })
            .await
            .unwrap();
        for route in ["/api/webui/config/bot", "/api/webui/config/model"] {
            Mock::given(method("POST"))
                .and(path(route))
                .and(header("cookie", "maibot_session=Ncd_tok"))
                .respond_with(
                    ResponseTemplate::new(200).set_body_json(serde_json::json!({"success": true})),
                )
                .mount(&server)
                .await;
        }

        let env = f.manager.read_config(&f.id).await.unwrap();
        let AppInstanceConfig::MaiBot(mut cfg) = env.config.clone() else {
            panic!("expected MaiBot config");
        };
        cfg.bot.personality.personality = "新人格".into();
        cfg.models.api_providers[0].api_key = "sk-live".into();
        let res = f
            .manager
            .write_config(&f.id, AppInstanceConfig::MaiBot(cfg), Some(env.revision))
            .await
            .unwrap();
        assert!(!res.restart_required, "人格、模型上游热加载，不用重启");
        assert_eq!(
            std::fs::read_to_string(&bot_path).unwrap(),
            seed,
            "运行中写交给 WebUI，桌面端不直接改文件"
        );
        let webui: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(f.inst_dir.join("data/webui.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(
            webui["first_setup_completed"], true,
            "模型配好了，WebUI 不必再走首次向导"
        );
        assert_eq!(webui["access_token"], "Ncd_tok");

        let body_of = |p: &str, reqs: &[wiremock::Request]| -> serde_json::Value {
            let req = reqs
                .iter()
                .rev()
                .find(|r| r.url.path() == p)
                .expect("该路由收到过请求");
            serde_json::from_slice(&req.body).unwrap()
        };
        let reqs = server.received_requests().await.unwrap();
        assert_eq!(
            body_of("/api/webui/config/bot", &reqs),
            serde_json::json!({"personality": {"personality": "新人格"}}),
            "bot 只带改了的键"
        );
        let model = body_of("/api/webui/config/model", &reqs);
        assert_eq!(model["api_providers"][0]["api_key"], "sk-live");
        assert!(
            model["models"].as_array().is_some_and(|m| !m.is_empty()),
            "model 整份给"
        );
        assert!(
            model["models"][0]["extra_params"].is_object(),
            "extra_params 交出去要是表"
        );

        let env = f.manager.read_config(&f.id).await.unwrap();
        let AppInstanceConfig::MaiBot(mut cfg) = env.config.clone() else {
            panic!("expected MaiBot config");
        };
        cfg.bot.maim_message.ws_server_port = 23012;
        let t0 = std::time::Instant::now();
        let res = f
            .manager
            .write_config(&f.id, AppInstanceConfig::MaiBot(cfg), Some(env.revision))
            .await
            .unwrap();
        assert!(
            res.restart_required,
            "旧版消息口启动时才绑，走接口写也得重启"
        );
        assert!(
            t0.elapsed() >= Duration::from_millis(1300),
            "两次写之间要留出上游热加载的间隔，实际 {:?}",
            t0.elapsed()
        );
        let reqs = server.received_requests().await.unwrap();
        assert_eq!(
            body_of("/api/webui/config/bot", &reqs),
            serde_json::json!({"maim_message": {"ws_server_port": 23012}})
        );
    }

    #[tokio::test]
    async fn maibot_runtime_calls_route_by_saved_or_draft_provider() {
        use wiremock::matchers::{header, method, path, query_param};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let f = maibot_fixture("m-rt").await;
        assert_eq!(
            f.manager.maibot_status(&f.id).await.unwrap().gate,
            MaiBotRuntimeGate::NotRunning,
            "没在跑不发请求，直接折成 gate"
        );
        assert!(matches!(
            f.manager.maibot_stats(&f.id, 24).await.unwrap_err(),
            AppFrameworkError::NotRunning(_)
        ));

        let server = MockServer::start().await;
        let port = server.address().port();
        std::fs::write(
                f.inst_dir.join("config/bot_config.toml"),
                format!("[inner]\nversion = \"8.14.40\"\n\n[webui]\nport = {port}\n\n[maim_message]\nws_server_port = 23002\n"),
            )
            .unwrap();
        f.manager
            .store
            .update(&f.id, |i| {
                i.port = port;
                i.state = AppInstanceState::Running;
            })
            .await
            .unwrap();
        let ok = |body: serde_json::Value| ResponseTemplate::new(200).set_body_json(body);
        Mock::given(method("GET"))
                .and(path("/api/webui/system/status"))
                .and(header("cookie", "maibot_session=Ncd_tok"))
                .respond_with(ok(serde_json::json!({"running": true, "uptime": 90.5, "version": "1.2.5", "start_time": "x"})))
                .mount(&server)
                .await;
        Mock::given(method("GET"))
            .and(path("/api/webui/models/list"))
            .and(query_param("provider_name", "DeepSeek"))
            .respond_with(ok(
                serde_json::json!({"success": true, "models": [{"id": "deepseek-chat"}]}),
            ))
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path("/api/webui/models/list-by-url"))
            .and(query_param("base_url", "https://draft.example/v1"))
            .respond_with(
                ResponseTemplate::new(502)
                    .set_body_json(serde_json::json!({"detail": "API Key 无效或已过期"})),
            )
            .mount(&server)
            .await;

        let status = f.manager.maibot_status(&f.id).await.unwrap();
        assert_eq!(status.gate, MaiBotRuntimeGate::Ok);
        assert_eq!(status.version.as_deref(), Some("1.2.5"));
        assert_eq!(status.uptime_secs, Some(90.5));

        let env = f.manager.read_config(&f.id).await.unwrap();
        let AppInstanceConfig::MaiBot(cfg) = env.config else {
            panic!("expected MaiBot config");
        };
        let saved = cfg.models.api_providers[0].clone();
        let models = f
            .manager
            .maibot_provider_models(&f.id, saved.clone())
            .await
            .unwrap();
        assert_eq!(
            models[0].id, "deepseek-chat",
            "盘上一模一样的按名字查，上游放行它自己配的内网地址"
        );

        let mut draft = saved;
        draft.base_url = "https://draft.example/v1".into();
        let err = f
            .manager
            .maibot_provider_models(&f.id, draft)
            .await
            .unwrap_err();
        assert!(
            matches!(&err, AppFrameworkError::Integration(m) if m == "API Key 无效或已过期"),
            "改过没保存的按地址查；上游 502 的原话直接给用户：{err:?}"
        );
    }

    /// 口和 token 读一次记着（远端现读一遍要十几次 SFTP）；麦麦那边换了 token，
    /// 详情页轮询的状态接口先撞上 401，作废后下一轮按盘上的新 token 恢复
    #[tokio::test]
    async fn maibot_webui_endpoint_is_cached_and_heals_after_auth_failure() {
        use wiremock::matchers::{header, method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let f = maibot_fixture("m-cache").await;
        let server = MockServer::start().await;
        let port = server.address().port();
        std::fs::write(
                f.inst_dir.join("config/bot_config.toml"),
                format!("[inner]\nversion = \"8.14.40\"\n\n[webui]\nport = {port}\n\n[maim_message]\nws_server_port = 23002\n"),
            )
            .unwrap();
        f.manager
            .store
            .update(&f.id, |i| {
                i.port = port;
                i.state = AppInstanceState::Running;
            })
            .await
            .unwrap();
        let status_ok = serde_json::json!({"running": true, "uptime": 1.0, "version": "1.2.5", "start_time": "x"});
        let accept = |token: &str| {
            Mock::given(method("GET"))
                .and(path("/api/webui/system/status"))
                .and(header("cookie", format!("maibot_session={token}").as_str()))
                .respond_with(ResponseTemplate::new(200).set_body_json(status_ok.clone()))
                .with_priority(1)
        };
        let reject = || {
            Mock::given(method("GET"))
                .and(path("/api/webui/system/status"))
                .respond_with(ResponseTemplate::new(401))
                .with_priority(10)
        };
        accept("Ncd_tok").mount(&server).await;
        reject().mount(&server).await;
        assert_eq!(
            f.manager.maibot_status(&f.id).await.unwrap().gate,
            MaiBotRuntimeGate::Ok
        );

        let webui_json = f.inst_dir.join("data/webui.json");
        std::fs::write(
            &webui_json,
            "{\"access_token\":\"Ncd_new\",\"token_source\":\"configured\"}\n",
        )
        .unwrap();
        assert_eq!(
            f.manager.webui_auth_key(&f.id).await,
            "Ncd_tok",
            "记着的不因为盘上变了就现读"
        );

        server.reset().await;
        accept("Ncd_new").mount(&server).await;
        reject().mount(&server).await;
        assert_eq!(
            f.manager.maibot_status(&f.id).await.unwrap().gate,
            MaiBotRuntimeGate::Auth
        );
        assert_eq!(
            f.manager.maibot_status(&f.id).await.unwrap().gate,
            MaiBotRuntimeGate::Ok
        );
        assert_eq!(f.manager.webui_auth_key(&f.id).await, "Ncd_new");
    }

    // 插件任务的标题取框架清单里的名字，去重键按实例、资源、名字、动作拼
    #[tokio::test]
    async fn store_op_task_title_and_dedupe_key() {
        let f = fixture(false).await;
        // 没装好的实例：任务一跑就停在「尚未安装」，不会去拉插件目录
        f.manager
            .store
            .update(&f.id, |i| i.state = AppInstanceState::NotInstalled)
            .await
            .unwrap();
        let tasks = f.manager.task_queue().unwrap().clone();
        let task_id = f
            .manager
            .submit_store_op(
                &f.id,
                "karin-plugin-foo",
                AppPluginAction::Uninstall,
                AppStoreResource::Plugin,
            )
            .await
            .unwrap();
        let list = tasks.list().await;
        let task = list.tasks.iter().find(|t| t.task_id == task_id).unwrap();
        assert_eq!(task.title, "Karin · 卸载插件 karin-plugin-foo");
        assert_eq!(
            task.dedupe_key.as_deref(),
            Some("app-plugin:k1:plugin:karin-plugin-foo:uninstall")
        );
    }

    // 落盘读不到账号、桌面端也没记用户名：AstrBot 用它首启写的默认用户名，没这个约定的框架给空
    #[tokio::test]
    async fn webui_login_username_falls_back_to_framework_default() {
        let f = fixture(false).await;
        let karin = f.manager.get_instance(&f.id).await.unwrap();
        let mut astrbot = karin.clone();
        astrbot.id = AppInstanceId::new("a1");
        astrbot.framework_id = AppFrameworkId::new("astrbot");
        assert_eq!(f.manager.webui_login_username(&astrbot).await, "astrbot");
        assert_eq!(f.manager.webui_login_username(&karin).await, "");
    }

    #[tokio::test]
    async fn creating_maibot_requires_accepting_terms() {
        let f = maibot_fixture("m6").await;
        let req = CreateAppInstanceRequest {
            framework_id: AppFrameworkId::new("maibot"),
            host_id: "local".into(),
            display_name: "新麦麦".into(),
            port: None,
            install_dir: None,
            install_renderer: None,
            webui_username: None,
            webui_password: None,
            auto_start: false,
            accept_terms: None,
        };
        let err = f.manager.create_instance(req.clone()).await.unwrap_err();
        assert!(err.to_string().contains("同意"), "{err}");
        let created = f
            .manager
            .create_instance(CreateAppInstanceRequest {
                accept_terms: Some(true),
                ..req
            })
            .await
            .unwrap();
        assert_eq!(created.framework_id.as_str(), "maibot");
    }

    fn karin(envelope: &AppInstanceConfigEnvelope) -> KarinInstanceConfig {
        let AppInstanceConfig::Karin(k) = &envelope.config else {
            panic!("expected Karin config");
        };
        k.clone()
    }

    fn client_url(plan: &OneBotLinkPlan) -> &str {
        &plan
            .connection
            .as_ws_client()
            .expect("Karin 是反向对接")
            .url
    }

    fn ws_client_urls(bot: &BotConfig) -> Vec<String> {
        bot.connect
            .websocket_clients
            .iter()
            .map(|c: &WebsocketClientConfig| c.url.clone())
            .collect()
    }

    #[tokio::test]
    async fn tail_log_reads_ncd_stdout_file() {
        let f = fixture(false).await;
        std::fs::write(
            f.inst_dir.join(".ncd-karin.log"),
            "boot\n[INFO] karin listening\n",
        )
        .unwrap();
        let snap = f.manager.tail_log(&f.id, 100).await.unwrap();
        assert!(
            snap.lines.iter().any(|l| l.contains("karin listening")),
            "{:?}",
            snap.lines
        );
    }

    #[tokio::test]
    async fn tail_log_falls_back_to_project_logs_dir() {
        let f = fixture(false).await;
        std::fs::create_dir_all(f.inst_dir.join("logs")).unwrap();
        std::fs::write(
            f.inst_dir.join("logs").join("nonebot.log"),
            "from logs dir\n",
        )
        .unwrap();
        let snap = f.manager.tail_log(&f.id, 100).await.unwrap();
        assert!(
            snap.lines.iter().any(|l| l.contains("from logs dir")),
            "{:?}",
            snap.lines
        );
    }

    #[tokio::test]
    async fn stale_revision_is_rejected_and_none_means_overwrite() {
        let f = fixture(false).await;
        let env = f.manager.read_config(&f.id).await.unwrap();
        let mut cfg = karin(&env);
        cfg.config.master.push("123".into());

        let err = f
            .manager
            .write_config(
                &f.id,
                AppInstanceConfig::Karin(cfg.clone()),
                Some("deadbeef".into()),
            )
            .await
            .unwrap_err();
        assert!(matches!(err, AppFrameworkError::ConfigConflict(_)));

        let ok = f
            .manager
            .write_config(&f.id, AppInstanceConfig::Karin(cfg.clone()), None)
            .await
            .unwrap();
        assert!(!ok.restart_required);
        assert!(!ok.relinked);
        assert!(!ok.port_changed);
        assert_ne!(ok.revision, env.revision);

        // 正确的 base_revision 通过
        let mut cfg2 = cfg.clone();
        cfg2.env.log_level = "debug".into();
        f.manager
            .write_config(&f.id, AppInstanceConfig::Karin(cfg2), Some(ok.revision))
            .await
            .unwrap();
        let again = karin(&f.manager.read_config(&f.id).await.unwrap());
        assert_eq!(again.env.log_level, "debug");
        assert_eq!(again.config.master, vec!["console", "123"]);
    }

    #[tokio::test]
    async fn port_change_syncs_instance_and_relinks_bot() {
        let f = fixture(true).await;
        let env = f.manager.read_config(&f.id).await.unwrap();
        let mut cfg = karin(&env);
        cfg.env.http_port = 7801;

        let res = f
            .manager
            .write_config(&f.id, AppInstanceConfig::Karin(cfg), Some(env.revision))
            .await
            .unwrap();
        assert!(res.port_changed);
        assert!(res.relinked);
        assert!(!res.restart_required, "实例未运行，不提示重启");

        let inst = f.manager.get_instance(&f.id).await.unwrap();
        assert_eq!(inst.port, 7801);
        assert_eq!(*f.bots.upserts.lock().unwrap(), 1);
        let bot = f
            .bots
            .bot_config(&BotId::new("10001"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            ws_client_urls(&bot),
            vec!["ws://127.0.0.1:7801/onebot/v11/ws".to_string()]
        );
        assert_eq!(bot.connect.websocket_clients[0].base.token, "tok-1");
    }

    #[tokio::test]
    async fn ws_key_change_relinks_with_new_token_but_plain_edit_does_not() {
        let f = fixture(true).await;
        let env = f.manager.read_config(&f.id).await.unwrap();
        let mut cfg = karin(&env);
        cfg.config.admin.push("42".into());
        let res = f
            .manager
            .write_config(&f.id, AppInstanceConfig::Karin(cfg), Some(env.revision))
            .await
            .unwrap();
        assert!(!res.relinked);
        assert_eq!(*f.bots.upserts.lock().unwrap(), 0);

        let mut cfg = karin(&f.manager.read_config(&f.id).await.unwrap());
        cfg.env.ws_server_auth_key = "tok-2".into();
        let res = f
            .manager
            .write_config(&f.id, AppInstanceConfig::Karin(cfg), Some(res.revision))
            .await
            .unwrap();
        assert!(res.relinked);
        let bot = f
            .bots
            .bot_config(&BotId::new("10001"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bot.connect.websocket_clients[0].base.token, "tok-2");
    }

    #[tokio::test]
    async fn raw_text_write_checks_revision_and_json_syntax() {
        let f = fixture(false).await;
        let docs = f.manager.list_config_documents(&f.id).await.unwrap();
        assert_eq!(docs.len(), 7);
        let redis = f.manager.read_config_text(&f.id, "redis").await.unwrap();
        assert!(redis.text.contains("6379"));

        let bad = f
            .manager
            .write_config_text(&f.id, "redis", "{nope", Some(redis.revision.clone()))
            .await
            .unwrap_err();
        assert!(matches!(bad, AppFrameworkError::ConfigInvalid(_)));

        let stale = f
            .manager
            .write_config_text(&f.id, "redis", "{}", Some("old".into()))
            .await
            .unwrap_err();
        assert!(matches!(stale, AppFrameworkError::ConfigConflict(_)));

        let ok = f
            .manager
            .write_config_text(
                &f.id,
                "redis",
                "{\"url\":\"redis://10.0.0.1:6379\"}\n",
                Some(redis.revision),
            )
            .await
            .unwrap();
        assert_ne!(ok.revision, "missing");
        let cfg = karin(&f.manager.read_config(&f.id).await.unwrap());
        assert_eq!(cfg.redis.url, "redis://10.0.0.1:6379");

        // 缺失文件读出来是空文本 + missing
        let groups = f.manager.read_config_text(&f.id, "groups").await.unwrap();
        assert_eq!(groups.revision, "missing");
        assert_eq!(groups.text, "");
    }

    #[tokio::test]
    async fn unlink_then_relink_rewrites_bot_connection_and_fills_ws_server() {
        let f = fixture(false).await;
        std::fs::write(
            f.inst_dir.join("@karinjs/config/adapter.json"),
            r#"{"console":{"isLocal":true},"onebot":{}}"#,
        )
        .unwrap();

        f.manager
            .apply_link(&f.id, &BotId::new("10001"))
            .await
            .unwrap();
        let bot = f
            .bots
            .bot_config(&BotId::new("10001"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bot.connect.websocket_clients.len(), 1);
        assert_eq!(bot.connect.websocket_clients[0].base.name, "ncd-app:k1");
        assert_eq!(bot.connect.websocket_clients[0].base.token, "tok-1");

        f.manager.unlink(&f.id).await.unwrap();
        let bot = f
            .bots
            .bot_config(&BotId::new("10001"))
            .await
            .unwrap()
            .unwrap();
        assert!(bot.connect.websocket_clients.is_empty());
        assert!(f.manager.get_instance(&f.id).await.unwrap().link.is_none());

        f.manager
            .apply_link(&f.id, &BotId::new("10001"))
            .await
            .unwrap();
        let bot = f
            .bots
            .bot_config(&BotId::new("10001"))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(bot.connect.websocket_clients.len(), 1);
        assert_eq!(bot.connect.websocket_clients[0].base.token, "tok-1");
        assert!(f.manager.get_instance(&f.id).await.unwrap().link.is_some());

        let adapter: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(f.inst_dir.join("@karinjs/config/adapter.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(adapter["onebot"]["ws_server"]["enable"], true);
    }

    #[tokio::test]
    async fn preview_link_allows_local_nc_and_sl_to_remote_app() {
        let f = fixture_on_host(false, "remote:vps", AppPlacement::RemoteNative).await;
        for bot_id in ["10001", "20002"] {
            let plan = f
                .manager
                .preview_link(&f.id, &BotId::new(bot_id))
                .await
                .unwrap();
            assert_eq!(client_url(&plan), "ws://127.0.0.1:7777/onebot/v11/ws");
        }
    }

    #[tokio::test]
    async fn apply_link_local_bot_remote_app_needs_ssh_tunnel() {
        let f = fixture_on_host(false, "remote:vps", AppPlacement::RemoteNative).await;
        for bot_id in ["10001", "20002"] {
            let err = f
                .manager
                .apply_link(&f.id, &BotId::new(bot_id))
                .await
                .unwrap_err();
            let msg = err.to_string();
            assert!(msg.contains("open_tunnel"), "NC/SL 都应走同一条隧道：{msg}");
        }
    }

    #[tokio::test]
    async fn preview_link_allows_remote_nc_and_sl_to_local_app() {
        let f = fixture(false).await;
        for bot_id in ["30003", "40004"] {
            let plan = f
                .manager
                .preview_link(&f.id, &BotId::new(bot_id))
                .await
                .unwrap();
            assert_eq!(client_url(&plan), "ws://127.0.0.1:7777/onebot/v11/ws");
        }
    }

    #[tokio::test]
    async fn apply_link_remote_bot_local_app_needs_ssh_tunnel() {
        let f = fixture(false).await;
        for bot_id in ["30003", "40004"] {
            let err = f
                .manager
                .apply_link(&f.id, &BotId::new(bot_id))
                .await
                .unwrap_err();
            let msg = err.to_string();
            assert!(
                msg.contains("open_tunnel"),
                "远端 NC/SL 都应走同一条 -R：{msg}"
            );
        }
    }

    #[tokio::test]
    async fn preview_link_allows_two_remote_hosts() {
        let f = fixture_on_host(false, "remote:vps", AppPlacement::RemoteNative).await;
        let plan = f
            .manager
            .preview_link(&f.id, &BotId::new("50005"))
            .await
            .unwrap();
        assert_eq!(client_url(&plan), "ws://127.0.0.1:7777/onebot/v11/ws");
    }

    #[tokio::test]
    async fn apply_link_two_remotes_needs_bot_ssh_dial() {
        let f = fixture_on_host(false, "remote:vps", AppPlacement::RemoteNative).await;
        let err = f
            .manager
            .apply_link(&f.id, &BotId::new("50005"))
            .await
            .unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("SSH") || msg.contains("Linux"),
            "P2 应进入常驻隧道而不是桌面 open_tunnel: {msg}"
        );
        assert!(!msg.contains("尚未开放"), "{msg}");
        assert!(!msg.contains("open_tunnel"), "{msg}");
    }

    /// 真机冒烟，默认不跑：真下 MaiBot 源码、uv sync、对接、起、停一遍，要联网、几百 MB。
    ///   NCD_MAIBOT_SMOKE_DIR=D:/somewhere cargo test -p ncd-runtime --lib maibot_real_smoke -- --ignored --nocapture
    /// Bot 侧用一个只收握手头的 TCP 监听顶替 NapCat，看适配器是不是带着对的 token 连过来。
    #[tokio::test]
    #[ignore = "要联网下载 MaiBot 并起真进程，设 NCD_MAIBOT_SMOKE_DIR 后手动跑"]
    #[allow(clippy::print_stderr)] // 这个测试就是给人看的报告，靠 --nocapture 打出来
    async fn maibot_real_smoke() {
        use ncd_appframework::maibot::MaiBotComponent;
        use ncd_component::{ActionCtx, Component, DetectOutcome, ProgressKind};
        use std::time::{Duration, Instant};
        use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System, UpdateKind};
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        fn stamp(t0: Instant) -> String {
            format!("[{:>7.1}s]", t0.elapsed().as_secs_f32())
        }

        // cwd 或命令行落在实例目录里的 python，加上它们的子孙（子进程不能早于父进程，防 ppid 复用）。
        // 起点只认 python：在实例目录里开过的 shell 也是这个 cwd，连带把 cargo 和测试自己扫进来
        fn related(dir: &str) -> Vec<(u32, u64, String)> {
            let mut sys = System::new();
            sys.refresh_processes_specifics(
                ProcessesToUpdate::All,
                ProcessRefreshKind::new()
                    .with_cwd(UpdateKind::Always)
                    .with_cmd(UpdateKind::Always),
            );
            let norm = |s: &str| s.replace('\\', "/").trim_end_matches('/').to_lowercase();
            let want = norm(dir);
            let cmd_of = |p: &sysinfo::Process| {
                p.cmd()
                    .iter()
                    .map(|s| s.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            let mut hit: Vec<u32> = sys
                .processes()
                .iter()
                .filter(|(_, p)| {
                    let is_python = p
                        .name()
                        .to_string_lossy()
                        .to_lowercase()
                        .starts_with("python");
                    let cwd = p
                        .cwd()
                        .map(|c| norm(&c.to_string_lossy()))
                        .unwrap_or_default();
                    is_python && (cwd == want || norm(&cmd_of(p)).contains(&want))
                })
                .map(|(pid, _)| pid.as_u32())
                .collect();
            loop {
                let before = hit.len();
                for (pid, p) in sys.processes() {
                    let pid = pid.as_u32();
                    let Some(parent) = p.parent().and_then(|pp| sys.process(pp)) else {
                        continue;
                    };
                    if !hit.contains(&pid)
                        && hit.contains(&parent.pid().as_u32())
                        && p.start_time() >= parent.start_time()
                    {
                        hit.push(pid);
                    }
                }
                if hit.len() == before {
                    break;
                }
            }
            hit.into_iter()
                .filter_map(|pid| {
                    sys.process(Pid::from_u32(pid))
                        .map(|p| (pid, p.start_time(), cmd_of(p)))
                })
                .collect()
        }

        let base = std::path::PathBuf::from(
            std::env::var("NCD_MAIBOT_SMOKE_DIR").expect("设 NCD_MAIBOT_SMOKE_DIR"),
        );
        // 只清自己建过的目录（带标记文件才删），环境变量指错了也不会误删别的东西
        let work = base.join("ncd-maibot-smoke");
        let marker = work.join(".ncd-smoke");
        if work.exists() {
            assert!(
                marker.is_file(),
                "{} 不是冒烟测试建的目录，不动它",
                work.display()
            );
            std::fs::remove_dir_all(&work).unwrap();
        }
        std::fs::create_dir_all(&work).unwrap();
        std::fs::write(&marker, b"").unwrap();
        let inst_dir = work.join("inst");
        let root = work.join("data");
        std::fs::create_dir_all(&root).unwrap();
        const WEBUI: u16 = 23901;
        let t0 = Instant::now();
        let local: Arc<dyn Host> = Arc::new(ncd_host::local::LocalWindowsHost::new());
        let install_dir = HostPath::from_windows(inst_dir.to_string_lossy().as_ref());
        let dir_str = inst_dir.to_string_lossy().to_string();

        let component = MaiBotComponent::new(install_dir.clone(), WEBUI);
        let (mut ctx, mut rx) = ActionCtx::new();
        let printer = tokio::spawn(async move {
            let mut last: Option<(u32, u8)> = None;
            while let Some(ev) = rx.recv().await {
                if let ProgressKind::StepProgress { step, percent, .. } = &ev.kind {
                    let bucket = percent / 20;
                    if last == Some((*step, bucket)) {
                        continue;
                    }
                    last = Some((*step, bucket));
                }
                eprintln!("{} {}", stamp(t0), serde_json::to_string(&ev.kind).unwrap());
            }
        });
        let installed = component.install(local.as_ref(), &mut ctx).await;
        drop(ctx);
        let _ = printer.await;
        installed.expect("安装失败");
        let outcome = component.detect_outcome(local.as_ref()).await.unwrap();
        eprintln!("{} 探测: {outcome:?}", stamp(t0));
        let DetectOutcome::Installed(version) = outcome else {
            panic!("装完探测不到");
        };

        let bus = Arc::new(BroadcastEventBus::default());
        let store = Arc::new(AppInstanceStore::empty(&root));
        let bots = Arc::new(MemoryBots {
            bots: AsyncMutex::new(vec![bot()]),
            upserts: Mutex::new(0),
        });
        let manager = Arc::new(AppManager::new(
            Arc::new(AppFrameworkRegistry::with_builtin()),
            Arc::clone(&store),
            Arc::new(NativeAppRuntime::new(Arc::clone(&bus), Arc::clone(&store))),
            Arc::new(ncd_server::LocalOnlyHostResolver::new(Arc::clone(&local))),
            bots.clone(),
            bus,
            &root,
        ));
        let id = AppInstanceId::new("smoke1");
        store
            .upsert(AppInstance {
                id: id.clone(),
                framework_id: AppFrameworkId::new("maibot"),
                display_name: "麦麦冒烟".into(),
                placement: AppPlacement::LocalNative,
                host_id: LOCAL_HOST_ID.to_string(),
                install_dir: install_dir.as_posix().to_string(),
                port: WEBUI,
                state: AppInstanceState::Stopped,
                link: None,
                installed_version: Some(version.version.clone()),
                last_error: None,
                created_at_ms: 1,
                install_renderer: false,
                origin: ncd_domain::AppInstanceOrigin::Created,
                auto_start: false,
            })
            .await
            .unwrap();

        let pending = manager.pending_terms(&id).await.unwrap();
        assert!(
            pending.is_empty(),
            "首装应已写好协议确认，还剩: {:?}",
            pending.iter().map(|p| p.id.clone()).collect::<Vec<_>>()
        );
        let AppInstanceConfig::MaiBot(cfg) = manager.read_config(&id).await.unwrap().config else {
            panic!("不是 MaiBot 配置");
        };
        eprintln!(
            "{} 配置: webui={} legacy={} token 长度={} adapter={:?}",
            stamp(t0),
            cfg.webui_port(),
            cfg.legacy_ws_port(),
            cfg.webui_token.len(),
            cfg.adapter.as_ref().map(|a| (a.enabled, a.napcat_port))
        );

        let linked = manager
            .apply_link(&id, &BotId::new("10001"))
            .await
            .expect("对接失败");
        let server = bots.bots.lock().await[0]
            .connect
            .websocket_servers
            .iter()
            .find(|s| s.base.name == app_link_connection_name(&id))
            .cloned()
            .expect("Bot 侧应多一条 WS 服务");
        eprintln!(
            "{} 对接: {:?}，Bot 侧听 {}:{}",
            stamp(t0),
            linked.link.as_ref().map(|l| &l.mode),
            server.host,
            server.port
        );

        let listener = tokio::net::TcpListener::bind(("127.0.0.1", server.port))
            .await
            .unwrap();
        let seen: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));
        let seen_in = Arc::clone(&seen);
        let fake_bot = tokio::spawn(async move {
            while let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = vec![0u8; 8192];
                let mut len = 0;
                while len < buf.len() {
                    match tokio::time::timeout(Duration::from_secs(5), sock.read(&mut buf[len..]))
                        .await
                    {
                        Ok(Ok(n)) if n > 0 => {
                            len += n;
                            if buf[..len].windows(4).any(|w| w == b"\r\n\r\n") {
                                break;
                            }
                        }
                        _ => break,
                    }
                }
                let head = String::from_utf8_lossy(&buf[..len]).to_string();
                let _ = sock
                    .write_all(b"HTTP/1.1 503 Service Unavailable\r\nContent-Length: 0\r\n\r\n")
                    .await;
                seen_in.lock().unwrap().get_or_insert(head);
            }
        });

        let started = manager.start_instance(&id).await;
        eprintln!(
            "{} 启动: {:?}",
            stamp(t0),
            started
                .as_ref()
                .map(|i| (&i.state, &i.last_error))
                .map_err(|e| e.to_string())
        );
        started.expect("启动失败");

        let mut webui_up = false;
        let deadline = Instant::now() + Duration::from_secs(420);
        let mut polls = 0u32;
        while Instant::now() < deadline {
            if tokio::net::TcpStream::connect(("127.0.0.1", WEBUI))
                .await
                .is_ok()
            {
                webui_up = true;
                break;
            }
            polls += 1;
            if polls % 5 == 0 && related(&dir_str).is_empty() {
                eprintln!("{} 进程没了", stamp(t0));
                break;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
        eprintln!("{} WebUI 口通: {webui_up}", stamp(t0));

        let mut index_ok = false;
        if webui_up {
            let client = reqwest::Client::new();
            // 前端静态资源来自 maibot-dashboard 包，缺了首页是空的，用户点「打开 WebUI」只看到 404
            match client
                .get(format!("http://127.0.0.1:{WEBUI}/"))
                .send()
                .await
            {
                Ok(r) => {
                    let status = r.status();
                    let body = r.text().await.unwrap_or_default();
                    index_ok = status.is_success() && body.to_ascii_lowercase().contains("<html");
                    let head: String = body.chars().take(120).collect();
                    eprintln!("{} 首页: {status} {head:?}", stamp(t0));
                }
                Err(e) => eprintln!("{} 首页: 请求失败 {e}", stamp(t0)),
            }
            let url = format!("http://127.0.0.1:{WEBUI}/api/webui/auth/verify");
            for (label, token) in [
                ("对的 token", cfg.webui_token.as_str()),
                ("错的 token", "wrong-token"),
            ] {
                match client
                    .post(&url)
                    .json(&serde_json::json!({ "token": token }))
                    .send()
                    .await
                {
                    Ok(r) => {
                        let status = r.status();
                        eprintln!(
                            "{} {label}: {status} {}",
                            stamp(t0),
                            r.text().await.unwrap_or_default()
                        );
                    }
                    Err(e) => eprintln!("{} {label}: 请求失败 {e}", stamp(t0)),
                }
            }
        }

        let deadline = Instant::now() + Duration::from_secs(120);
        while Instant::now() < deadline && seen.lock().unwrap().is_none() {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
        let head = seen.lock().unwrap().clone();
        match &head {
            Some(h) => eprintln!("{} 适配器握手:\n{h}", stamp(t0)),
            None => eprintln!("{} 适配器 120s 内没连过来", stamp(t0)),
        }
        let auth_ok = head.as_deref().is_some_and(|h| {
            h.lines().any(|l| {
                l.to_ascii_lowercase().starts_with("authorization:")
                    && l.contains(&server.base.token)
            })
        });
        let legacy_up = tokio::net::TcpStream::connect(("127.0.0.1", cfg.legacy_ws_port()))
            .await
            .is_ok();
        eprintln!(
            "{} 握手带对的 token: {auth_ok}，旧版消息口在听: {legacy_up}",
            stamp(t0)
        );

        let procs = related(&dir_str);
        eprintln!("{} 停之前的相关进程:", stamp(t0));
        for (pid, _, cmd) in &procs {
            eprintln!("  {pid} {cmd}");
        }
        let stopped = manager.stop_instance(&id).await;
        eprintln!(
            "{} 停止: {:?}",
            stamp(t0),
            stopped
                .as_ref()
                .map(|i| &i.state)
                .map_err(|e| e.to_string())
        );
        tokio::time::sleep(Duration::from_secs(3)).await;

        let mut sys = System::new();
        sys.refresh_processes_specifics(ProcessesToUpdate::All, ProcessRefreshKind::new());
        let leftovers: Vec<_> = procs
            .iter()
            .filter(|(pid, start, _)| {
                sys.process(Pid::from_u32(*pid))
                    .is_some_and(|p| p.start_time() == *start)
            })
            .collect();
        let after = related(&dir_str);
        let webui_after = tokio::net::TcpStream::connect(("127.0.0.1", WEBUI))
            .await
            .is_ok();
        let legacy_after = tokio::net::TcpStream::connect(("127.0.0.1", cfg.legacy_ws_port()))
            .await
            .is_ok();
        eprintln!(
            "{} 停后残留: {leftovers:?}，重扫: {after:?}，WebUI 口仍通: {webui_after}，旧版口仍通: {legacy_after}",
            stamp(t0)
        );

        let log = inst_dir.join(".ncd-maibot.log");
        let text = std::fs::read(&log)
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        let tail: Vec<&str> = text.lines().rev().take(80).collect();
        eprintln!("---- {} 末 80 行 ----", log.display());
        for line in tail.into_iter().rev() {
            eprintln!("{line}");
        }

        fake_bot.abort();
        assert!(webui_up, "WebUI 没起来");
        assert!(index_ok, "WebUI 首页打不开");
        assert!(auth_ok, "适配器没带对的 token 连过来");
        assert!(leftovers.is_empty() && after.is_empty(), "停后有残留进程");
        assert!(!webui_after && !legacy_after, "停后端口没释放");
    }
}

#[test]
fn utc_offset_from_date_z() {
    assert_eq!(parse_utc_offset("+0800\n"), Some(8 * 3600));
    assert_eq!(parse_utc_offset("+0000"), Some(0));
    assert_eq!(parse_utc_offset("-0530"), Some(-(5 * 3600 + 30 * 60)));
    assert_eq!(parse_utc_offset("CST"), None);
    assert_eq!(parse_utc_offset("+08:00"), None);
    assert_eq!(parse_utc_offset(""), None);
}

#[test]
fn parse_user_install_dir_windows_and_posix() {
    let p = parse_user_install_dir(r"D:\bots\karin-a", Os::Windows).unwrap();
    assert!(p.is_absolute());
    assert_eq!(p.as_posix(), "/d/bots/karin-a");
    assert!(parse_user_install_dir("relative/path", Os::Windows).is_err());
    let nix = parse_user_install_dir("/home/u/ncd/apps/karin/x", Os::Linux).unwrap();
    assert_eq!(nix.as_posix(), "/home/u/ncd/apps/karin/x");
}

#[cfg(windows)]
mod custom_install_dir {
    use super::*;
    use crate::events::BroadcastEventBus;
    use ncd_appframework::AppFrameworkRegistry;
    use ncd_host::local::LocalWindowsHost;

    async fn dir_manager(root: &std::path::Path) -> Arc<AppManager> {
        dir_manager_with_bots(root, Arc::new(MemoryBotsStub)).await
    }

    async fn dir_manager_with_bots(
        root: &std::path::Path,
        bots: Arc<dyn BotConfigPort>,
    ) -> Arc<AppManager> {
        let bus = Arc::new(BroadcastEventBus::default());
        let store = Arc::new(AppInstanceStore::empty(root));
        let local: Arc<dyn Host> = Arc::new(LocalWindowsHost::new());
        Arc::new(AppManager::new(
            Arc::new(AppFrameworkRegistry::with_builtin()),
            Arc::clone(&store),
            Arc::new(NativeAppRuntime::new(Arc::clone(&bus), Arc::clone(&store))),
            Arc::new(ncd_server::LocalOnlyHostResolver::new(local)),
            bots,
            bus,
            root,
        ))
    }

    struct MemoryBotsStub;

    #[async_trait::async_trait]
    impl BotConfigPort for MemoryBotsStub {
        async fn bot_config(&self, _: &BotId) -> Result<Option<BotConfig>, String> {
            Ok(None)
        }
        async fn upsert_bot_config(&self, _: BotConfig) -> Result<(), String> {
            Ok(())
        }
    }

    #[tokio::test]
    async fn custom_install_dir_rejects_relative_and_nonempty() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        let manager = dir_manager(&root).await;

        let rel = manager
            .resolve_install_dir_for_test(
                "local",
                &AppFrameworkId::new("karin"),
                &AppInstanceId::new("x"),
                Some("apps/foo"),
            )
            .await;
        assert!(matches!(rel, Err(AppFrameworkError::Validation(m)) if m.contains("绝对")));

        let occupied = tmp.path().join("taken");
        std::fs::create_dir_all(&occupied).unwrap();
        std::fs::write(occupied.join("keep.txt"), b"x").unwrap();
        let err = manager
            .resolve_install_dir_for_test(
                "local",
                &AppFrameworkId::new("karin"),
                &AppInstanceId::new("x"),
                Some(occupied.to_str().unwrap()),
            )
            .await;
        assert!(matches!(err, Err(AppFrameworkError::Validation(m)) if m.contains("非空")));
    }

    #[tokio::test]
    async fn custom_install_dir_rejects_collision() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        let manager = dir_manager(&root).await;
        let created = manager
            .create_instance(CreateAppInstanceRequest {
                framework_id: AppFrameworkId::new("karin"),
                host_id: "local".into(),
                display_name: "a".into(),
                port: Some(7777),
                install_dir: None,
                install_renderer: None,
                webui_username: None,
                webui_password: None,
                auto_start: true,
                accept_terms: None,
            })
            .await
            .unwrap();
        let err = manager
            .resolve_install_dir_for_test(
                "local",
                &AppFrameworkId::new("karin"),
                &AppInstanceId::new("other"),
                Some(&created.install_dir),
            )
            .await;
        assert!(matches!(err, Err(AppFrameworkError::Validation(m)) if m.contains("占用")));
    }

    #[tokio::test]
    async fn import_nonebot_adopts_nonempty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        let project = tmp.path().join("bot-xiuxian");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("pyproject.toml"),
            r#"
[project]
name = "bot-xiuxian"
dependencies = ["nonebot2[httpx,websockets]>=2.5.0"]

[tool.nonebot]
plugin_dirs = ["src/plugins"]
"#,
        )
        .unwrap();
        std::fs::write(
            project.join("bot.py"),
            "import nonebot\nfrom nonebot.adapters.onebot.v11 import Adapter\nnonebot.init()\n",
        )
        .unwrap();
        std::fs::write(
            project.join(".env"),
            "DRIVER=~httpx+~websockets\nPORT=13120\nONEBOT_WS_URLS=[\"ws://127.0.0.1:3001\"]\n",
        )
        .unwrap();
        let manager = dir_manager(&root).await;
        let imported = manager
            .import_instance(ImportAppInstanceRequest {
                framework_id: AppFrameworkId::new("nonebot2"),
                host_id: "local".into(),
                path: project.to_string_lossy().into_owned(),
                display_name: String::new(),
            })
            .await
            .unwrap();
        assert_eq!(imported.origin, AppInstanceOrigin::Imported);
        assert_eq!(imported.port, 13120);
        assert_eq!(imported.display_name, "bot-xiuxian");
        assert_eq!(imported.state, AppInstanceState::NotInstalled);

        let create_err = manager
            .create_instance(CreateAppInstanceRequest {
                framework_id: AppFrameworkId::new("nonebot2"),
                host_id: "local".into(),
                display_name: "x".into(),
                port: Some(20001),
                install_dir: Some(project.to_string_lossy().into_owned()),
                install_renderer: None,
                webui_username: None,
                webui_password: None,
                auto_start: true,
                accept_terms: None,
            })
            .await;
        assert!(
            matches!(create_err, Err(AppFrameworkError::Validation(ref m)) if m.contains("占用") || m.contains("非空")),
            "{create_err:?}"
        );

        std::fs::write(project.join(".env"), "PORT=1\nONEBOT_ACCESS_TOKEN=stolen\n").unwrap();
        std::fs::write(project.join(".ncd-uv"), "marker\n").unwrap();
        manager.delete_instance(&imported.id, false).await.unwrap();
        assert_eq!(
            std::fs::read_to_string(project.join(".env")).unwrap(),
            "DRIVER=~httpx+~websockets\nPORT=13120\nONEBOT_WS_URLS=[\"ws://127.0.0.1:3001\"]\n"
        );
        assert_eq!(
            std::fs::read_to_string(project.join("bot.py")).unwrap(),
            "import nonebot\nfrom nonebot.adapters.onebot.v11 import Adapter\nnonebot.init()\n"
        );
        assert!(!project.join(".ncd-uv").exists());
        assert!(!project.join(".env.ncd.bak").exists());
        assert!(manager.get_instance(&imported.id).await.is_err());
    }

    struct ForwardWsBots;

    #[async_trait::async_trait]
    impl BotConfigPort for ForwardWsBots {
        async fn bot_config(&self, bot_id: &BotId) -> Result<Option<BotConfig>, String> {
            Ok(self
                .list_bot_configs_for_link()
                .await?
                .into_iter()
                .find(|b| b.bot.qq_id.to_string() == bot_id.as_str()))
        }
        async fn upsert_bot_config(&self, _: BotConfig) -> Result<(), String> {
            Ok(())
        }
        async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
            let mut b = super::bot();
            b.connect.websocket_servers.push(WebsocketServerConfig {
                base: NetworkBaseFields {
                    enable: true,
                    name: "ws".into(),
                    message_post_format: MessagePostFormat::Array,
                    token: String::new(),
                    debug: false,
                },
                host: "0.0.0.0".into(),
                port: 3001,
                report_self_message: false,
                enable_force_push_event: false,
                heart_interval: 30000,
                path: "/".into(),
                role: WsRole::Universal,
            });
            Ok(vec![b])
        }
    }

    #[tokio::test]
    async fn import_nonebot_adopts_existing_forward_ws() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join("data");
        std::fs::create_dir_all(&root).unwrap();
        let project = tmp.path().join("bot-xiuxian");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("pyproject.toml"),
            r#"
[project]
name = "bot-xiuxian"
dependencies = ["nonebot2[httpx,websockets]>=2.5.0"]

[tool.nonebot]
plugin_dirs = ["src/plugins"]
"#,
        )
        .unwrap();
        std::fs::write(
            project.join("bot.py"),
            "import nonebot\nfrom nonebot.adapters.onebot.v11 import Adapter\nnonebot.init()\n",
        )
        .unwrap();
        std::fs::write(
            project.join(".env"),
            "DRIVER=~httpx+~websockets\nPORT=13120\nONEBOT_WS_URLS=[\"ws://127.0.0.1:3001\"]\n",
        )
        .unwrap();
        let manager = dir_manager_with_bots(&root, Arc::new(ForwardWsBots)).await;
        let probe = manager
            .probe_project(
                "local",
                &AppFrameworkId::new("nonebot2"),
                project.to_str().unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            probe.detected_bot_id.as_ref().map(|id| id.as_str()),
            Some("10001")
        );
        let imported = manager
            .import_instance(ImportAppInstanceRequest {
                framework_id: AppFrameworkId::new("nonebot2"),
                host_id: "local".into(),
                path: project.to_string_lossy().into_owned(),
                display_name: String::new(),
            })
            .await
            .unwrap();
        let link = imported.link.expect("should adopt existing bot");
        assert_eq!(link.bot_id.as_str(), "10001");
        assert_eq!(link.connection_name, ncd_domain::APP_LINK_ADOPTED_FORWARD);
    }
}
