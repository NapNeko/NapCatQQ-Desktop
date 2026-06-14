use ncd_runtime::remote_snowluma_layout::{RemoteSnowLumaLayout, SnowLumaRemotePaths};

fn sample_layout() -> RemoteSnowLumaLayout {
    let paths = SnowLumaRemotePaths::from_remote_home("/home/u");
    RemoteSnowLumaLayout {
        home: "/home/u".into(),
        paths,
        node_bin: "/home/u/snowluma-remote/workspace/node/bin/node".into(),
        qq_bin: "/home/u/Napcat/opt/QQ/qq".into(),
    }
}

#[test]
fn layout_qq_install_path_contract() {
    let layout = sample_layout();
    assert!(layout.qq_bin.contains("/home/u/Napcat/opt/QQ/qq"));
}

#[test]
fn stack_runtime_paths_contract() {
    let paths = SnowLumaRemotePaths::from_remote_home("/home/u");
    assert!(paths.status_daemon.contains("status_daemon.json"));
    assert!(paths.pid_daemon.contains("pid_daemon"));
}