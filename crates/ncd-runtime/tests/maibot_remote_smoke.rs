//! 麦麦远端真机冒烟，默认 #[ignore]：连一台 Linux（沿用 ncd-host 真机冒烟的那组环境变量），真装 uv 和麦麦、
//! 对接同机的「Bot」（服务器上起一个只收握手头的监听顶替 NapCat）、起、经 SSH 隧道打 WebUI、停，逐步打印。
//!
//! powershell
//! $env:NCD_TEST_SSH_HOST = "1.2.3.4"; $env:NCD_TEST_SSH_USER = "ubuntu"
//! $env:NCD_TEST_SSH_KEY = "$env:USERPROFILE\.ssh\id_ed25519"    （端口不是 22 再设 NCD_TEST_SSH_PORT）
//! cargo test -p ncd-runtime --test maibot_remote_smoke -- --ignored --nocapture
//!
//! 服务器上只动 ~/ncd-maibot-smoke（带标记文件才删了重来）和 ~/ncd/tools/uv；要联网、下几百 MB。
//! 只走 AppManager 的公开接口，所以不放在它的单元测试里。桌面端这一侧用的是本机 Windows Host

#![cfg(windows)]

use std::sync::Arc;

use ncd_appframework::MaiBotRuntimeGate;
use ncd_domain::{
    AdvancedConfig, AppFrameworkId, AppInstance, AppInstanceId, AppInstanceState, AppPlacement,
    AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, BotId, ConnectConfig,
    DeploymentType, RuntimeTarget,
};
use ncd_host::{Host, HostCommand, HostPath, Os};
use ncd_runtime::{
    AppFrameworkRegistry, AppInstanceConfig, AppInstanceStore, AppManager, BotConfigPort,
    BroadcastEventBus, HostResolveError, HostResolver, NativeAppRuntime,
};
use tokio::sync::Mutex as AsyncMutex;

struct MemoryBots {
    bots: AsyncMutex<Vec<BotConfig>>,
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
        Ok(())
    }

    async fn list_bot_configs_for_link(&self) -> Result<Vec<BotConfig>, String> {
        Ok(self.bots.lock().await.clone())
    }
}

/// 服务器上正在监听的口；读不到按没有算，这里只用来提示测试口被占
async fn remote_listening_ports(host: &dyn Host) -> Vec<u16> {
    ncd_appframework::remote_listening_ports(host)
        .await
        .unwrap_or_default()
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

#[tokio::test]
#[ignore = "要一台 Linux 服务器：设 NCD_TEST_SSH_HOST / NCD_TEST_SSH_USER / NCD_TEST_SSH_KEY 后手动跑"]
#[allow(clippy::print_stderr)] // 这个测试就是给人看的报告，靠 --nocapture 打出来
async fn maibot_remote_smoke() {
    use ncd_appframework::maibot::MaiBotComponent;
    use ncd_component::{
        ActionCtx, Component, DetectOutcome, ProgressKind, UV_DEFAULT_VERSION, UvComponent,
    };
    use ncd_host::remote::{ConnectionConfig, HostKeyPolicy, RemoteLinuxHost, SshCredentials};
    use ncd_host::shell_single_quote;
    use std::time::{Duration, Instant};

    fn stamp(t0: Instant) -> String {
        format!("[{:>7.1}s]", t0.elapsed().as_secs_f32())
    }

    async fn sh(host: &dyn Host, script: &str) -> String {
        host.run_to_string(
            HostCommand::new("sh")
                .arg("-c")
                .arg(script)
                .timeout(Duration::from_secs(60)),
        )
        .await
        .map(|o| o.stdout)
        .unwrap_or_default()
    }

    /// cwd 落在实例目录里的进程：Runner、Worker、插件 Runner
    async fn related(host: &dyn Host, dir: &str) -> Vec<String> {
        let script = format!(
            "d={}; for p in /proc/[0-9]*; do c=$(readlink \"$p/cwd\" 2>/dev/null) || continue; \
                     case \"$c\" in \"$d\"|\"$d\"/*) echo \"${{p#/proc/}} $(tr '\\0' ' ' < \"$p/cmdline\" 2>/dev/null)\";; esac; done",
            shell_single_quote(dir)
        );
        sh(host, &script)
            .await
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(str::to_string)
            .collect()
    }

    /// 进度逐条打出来；用完先 drop ctx 再等打印任务收尾
    fn progress(t0: Instant) -> (ActionCtx, tokio::task::JoinHandle<()>) {
        let (ctx, mut rx) = ActionCtx::new();
        let printer = tokio::spawn(async move {
            let mut last: Option<(u32, u8)> = None;
            while let Some(ev) = rx.recv().await {
                if let ProgressKind::StepProgress { step, percent, .. } = &ev.kind {
                    if last == Some((*step, percent / 20)) {
                        continue;
                    }
                    last = Some((*step, percent / 20));
                }
                eprintln!("{} {}", stamp(t0), serde_json::to_string(&ev.kind).unwrap());
            }
        });
        (ctx, printer)
    }

    struct SmokeResolver {
        local: Arc<dyn Host>,
        remote: Arc<dyn Host>,
    }

    #[async_trait::async_trait]
    impl HostResolver for SmokeResolver {
        async fn resolve(&self, target: &RuntimeTarget) -> Result<Arc<dyn Host>, HostResolveError> {
            Ok(if target.is_local() {
                Arc::clone(&self.local)
            } else {
                Arc::clone(&self.remote)
            })
        }
    }

    let t0 = Instant::now();
    let ssh_host = std::env::var("NCD_TEST_SSH_HOST").expect("设 NCD_TEST_SSH_HOST");
    let user = std::env::var("NCD_TEST_SSH_USER").unwrap_or_else(|_| "ubuntu".into());
    let key = std::env::var("NCD_TEST_SSH_KEY").expect("设 NCD_TEST_SSH_KEY");
    let ssh_port = std::env::var("NCD_TEST_SSH_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(22);
    let cfg = ConnectionConfig::new(
        ssh_host,
        ssh_port,
        SshCredentials::key_file(user, key, None),
        HostKeyPolicy::Insecure,
    )
    .with_connect_timeout(Duration::from_secs(20));
    let remote: Arc<dyn Host> = Arc::new(
        RemoteLinuxHost::connect("smoke", cfg)
            .await
            .expect("SSH 连不上"),
    );
    let system = sh(remote.as_ref(), "uname -m; (. /etc/os-release && echo \"$PRETTY_NAME\"); getconf GNU_LIBC_VERSION; df -h \"$HOME\" | tail -n 1").await;
    eprintln!("{} 服务器:\n{}", stamp(t0), system.trim());

    let home = sh(remote.as_ref(), "printf '%s' \"$HOME\"").await;
    assert!(home.starts_with('/'), "取不到远端 $HOME：{home:?}");
    let work = HostPath::from_posix(&home).join("ncd-maibot-smoke");
    let marker = work.join(".ncd-smoke");
    if remote.exists(&work).await.unwrap() {
        assert!(
            remote.exists(&marker).await.unwrap(),
            "{} 不是冒烟测试建的目录，不动它",
            work.as_posix()
        );
        remote.remove_dir_all(&work).await.unwrap();
    }
    remote.create_dir_all(&work).await.unwrap();
    remote.write_file(&marker, b"").await.unwrap();
    let install_dir = work.join("inst");

    let uv = UvComponent::new(
        UV_DEFAULT_VERSION,
        UvComponent::default_remote_install_dir(&home),
    );
    let uv_bin = UvComponent::uv_binary_path_for_os(&uv.install_dir, Os::Linux);
    if !remote.exists(&uv_bin).await.unwrap_or(false) {
        let (mut ctx, printer) = progress(t0);
        let done = uv.install(remote.as_ref(), &mut ctx).await;
        drop(ctx);
        let _ = printer.await;
        done.expect("装 uv 失败");
    }

    const WEBUI: u16 = 23911;
    assert!(
        !remote_listening_ports(remote.as_ref())
            .await
            .contains(&WEBUI),
        "服务器上 {WEBUI} 被占了，改一下测试里的口"
    );
    let component = MaiBotComponent::new(install_dir.clone(), WEBUI).with_uv_bin(Some(uv_bin));
    let (mut ctx, printer) = progress(t0);
    let installed = component.install(remote.as_ref(), &mut ctx).await;
    drop(ctx);
    let _ = printer.await;
    installed.expect("装麦麦失败");
    let outcome = component.detect_outcome(remote.as_ref()).await.unwrap();
    eprintln!("{} 探测: {outcome:?}", stamp(t0));
    let DetectOutcome::Installed(version) = outcome else {
        panic!("装完探测不到");
    };

    let tmp = tempfile::tempdir().unwrap();
    let bus = Arc::new(BroadcastEventBus::default());
    let store = Arc::new(AppInstanceStore::empty(tmp.path()));
    let bots = Arc::new(MemoryBots {
        bots: AsyncMutex::new(vec![bot_with(
            10001,
            BackendType::NapCat,
            RuntimeTarget::server("smoke"),
        )]),
    });
    let manager = Arc::new(AppManager::new(
        Arc::new(AppFrameworkRegistry::with_builtin()),
        Arc::clone(&store),
        Arc::new(NativeAppRuntime::new(Arc::clone(&bus), Arc::clone(&store))),
        Arc::new(SmokeResolver {
            local: Arc::new(ncd_host::local::LocalWindowsHost::new()),
            remote: Arc::clone(&remote),
        }),
        bots.clone(),
        bus,
        tmp.path(),
    ));
    let id = AppInstanceId::new("rsmoke1");
    store
        .upsert(AppInstance {
            id: id.clone(),
            framework_id: AppFrameworkId::new("maibot"),
            display_name: "远端麦麦冒烟".into(),
            placement: AppPlacement::RemoteNative,
            host_id: "remote:smoke".into(),
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
    assert!(
        manager.pending_terms(&id).await.unwrap().is_empty(),
        "首装应已写好协议确认"
    );
    let AppInstanceConfig::MaiBot(cfg) = manager.read_config(&id).await.unwrap().config else {
        panic!("不是 MaiBot 配置");
    };
    eprintln!(
        "{} 配置: webui={} legacy={} token 长度={}",
        stamp(t0),
        cfg.webui_port(),
        cfg.legacy_ws_port(),
        cfg.webui_token.len()
    );

    manager
        .apply_link(&id, &BotId::new("10001"))
        .await
        .expect("对接失败");
    let server = bots.bots.lock().await[0].connect.websocket_servers[0].clone();
    eprintln!(
        "{} 对接: Bot 侧听 {}:{}",
        stamp(t0),
        server.host,
        server.port
    );
    // 服务器上顶替 NapCat：只收握手头记下来，回个 503 让适配器过一会儿再连
    let fake_bot = "import socket,sys\nport=int(sys.argv[1]);out=sys.argv[2]\n\
                s=socket.socket();s.setsockopt(socket.SOL_SOCKET,socket.SO_REUSEADDR,1);s.bind(('127.0.0.1',port));s.listen(5)\n\
                while True:\n    c,_=s.accept();c.settimeout(5);d=b''\n    try:\n        \
                while b'\\r\\n\\r\\n' not in d and len(d)<8192:\n            k=c.recv(4096)\n            \
                if not k: break\n            d+=k\n    except Exception: pass\n    \
                open(out,'ab').write(d+b'\\n----\\n')\n    \
                try: c.sendall(b'HTTP/1.1 503 Service Unavailable\\r\\nContent-Length: 0\\r\\n\\r\\n')\n    \
                except Exception: pass\n    c.close()\n";
    remote
        .write_file(&work.join("fake_bot.py"), fake_bot.as_bytes())
        .await
        .unwrap();
    let python = component.venv_python(Os::Linux);
    let fake_pid = sh(
        remote.as_ref(),
        &format!(
            "cd {} && nohup setsid {} fake_bot.py {} handshake.txt >/dev/null 2>&1 & echo $!",
            shell_single_quote(work.as_posix()),
            shell_single_quote(python.as_posix()),
            server.port
        ),
    )
    .await;

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
    let inst = manager.get_instance(&id).await.unwrap();
    let local_port = manager
        .desktop_webui_loopback_port(&inst)
        .await
        .expect("开不了 WebUI 隧道");
    eprintln!(
        "{} WebUI 隧道: 本机 127.0.0.1:{local_port} -> 服务器 {WEBUI}",
        stamp(t0)
    );

    let client = reqwest::Client::new();
    let mut index_ok = false;
    let deadline = Instant::now() + Duration::from_secs(420);
    while Instant::now() < deadline {
        if let Ok(r) = client
            .get(format!("http://127.0.0.1:{local_port}/"))
            .send()
            .await
        {
            let status = r.status();
            let body = r.text().await.unwrap_or_default();
            if status.is_success() && body.to_ascii_lowercase().contains("<html") {
                index_ok = true;
                break;
            }
        }
        if related(remote.as_ref(), install_dir.as_posix())
            .await
            .is_empty()
        {
            eprintln!("{} 进程没了", stamp(t0));
            break;
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
    eprintln!("{} 经隧道打开首页: {index_ok}", stamp(t0));
    let verify = client
        .post(format!(
            "http://127.0.0.1:{local_port}/api/webui/auth/verify"
        ))
        .json(&serde_json::json!({ "token": cfg.webui_token }))
        .send()
        .await
        .map(|r| r.status().is_success())
        .unwrap_or(false);
    let status = manager.maibot_status(&id).await.unwrap();
    eprintln!(
        "{} token 校验: {verify}，运行状态: {:?} {}",
        stamp(t0),
        status.gate,
        status.message
    );

    let mut handshake = String::new();
    let deadline = Instant::now() + Duration::from_secs(120);
    while Instant::now() < deadline && !handshake.to_ascii_lowercase().contains("authorization") {
        handshake = sh(
            remote.as_ref(),
            &format!(
                "cat {} 2>/dev/null",
                shell_single_quote(work.join("handshake.txt").as_posix())
            ),
        )
        .await;
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
    let auth_ok = handshake.lines().any(|l| {
        l.to_ascii_lowercase().starts_with("authorization:") && l.contains(&server.base.token)
    });
    eprintln!(
        "{} 适配器握手带对的 token: {auth_ok}\n{}",
        stamp(t0),
        handshake.trim()
    );

    let before = related(remote.as_ref(), install_dir.as_posix()).await;
    eprintln!("{} 停之前的相关进程:\n  {}", stamp(t0), before.join("\n  "));
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
    let after = related(remote.as_ref(), install_dir.as_posix()).await;
    let listening = remote_listening_ports(remote.as_ref()).await;
    let ports_left: Vec<u16> = [WEBUI, cfg.legacy_ws_port()]
        .into_iter()
        .filter(|p| listening.contains(p))
        .collect();
    eprintln!(
        "{} 停后残留进程: {after:?}，还在听的口: {ports_left:?}",
        stamp(t0)
    );
    let _ = sh(
        remote.as_ref(),
        &format!("kill {} 2>/dev/null", fake_pid.trim()),
    )
    .await;

    let log = remote
        .read_file(&install_dir.join(".ncd-maibot.log"))
        .await
        .map(|b| String::from_utf8_lossy(&b).to_string())
        .unwrap_or_default();
    eprintln!("---- .ncd-maibot.log 末 60 行 ----");
    for line in log
        .lines()
        .rev()
        .take(60)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
    {
        eprintln!("{line}");
    }

    assert!(index_ok, "经隧道打不开 WebUI 首页");
    assert!(verify, "WebUI 不认 token");
    assert_eq!(status.gate, MaiBotRuntimeGate::Ok, "运行状态不对");
    assert!(auth_ok, "适配器没带对的 token 连过来");
    assert!(after.is_empty(), "停后有残留进程（按进程组停没收干净）");
    assert!(ports_left.is_empty(), "停后端口没释放");
}
