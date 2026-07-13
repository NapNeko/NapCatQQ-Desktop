use super::decision::{
    TunnelAction, decide_tunnel_action, health_force_retunnel, should_republish, should_retunnel,
};
use super::tunnel_io::scan_latest_webui;

#[test]
fn empty_log_returns_none() {
    assert!(scan_latest_webui(b"").is_none());
    assert!(scan_latest_webui(b"noise without panel url\n").is_none());
}

#[test]
fn keeps_last_webui_line_when_multiple() {
    let log = b"\
[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6099/webui?token=first\n\
noise\n\
[info] [NapCat] [WebUi] WebUi User Panel Url: http://127.0.0.1:6101/webui?token=second\n\
";
    let (port, token) = scan_latest_webui(log).expect("should parse");
    assert_eq!(port, 6101);
    assert_eq!(token, "second");
}

#[test]
fn loose_url_fragment_also_works() {
    let log = b"panel http://127.0.0.1:6123/webui?token=loose_tok trailing\n";
    let (port, token) = scan_latest_webui(log).expect("loose parse");
    assert_eq!(port, 6123);
    assert_eq!(token, "loose_tok");
}

#[test]
fn should_retunnel_when_no_local_port() {
    assert!(should_retunnel(None, false, 6100));
}

#[test]
fn should_retunnel_when_remote_port_changed() {
    assert!(should_retunnel(Some(6099), true, 6100));
}

#[test]
fn should_not_retunnel_when_same_remote_and_alive() {
    assert!(!should_retunnel(Some(6100), true, 6100));
}

#[test]
fn should_republish_when_token_changes() {
    let last = (50000_u16, 6100_u16, "old".to_string());
    assert!(should_republish(Some(&last), 50000, 6100, "new"));
}

#[test]
fn should_not_republish_when_all_same() {
    let last = (50000_u16, 6100_u16, "tok".to_string());
    assert!(!should_republish(Some(&last), 50000, 6100, "tok"));
}

#[test]
fn should_republish_when_local_port_changes() {
    let last = (50000_u16, 6100_u16, "tok".to_string());
    assert!(should_republish(Some(&last), 50001, 6100, "tok"));
}

#[test]
fn should_republish_when_remote_port_changes() {
    let last = (50000_u16, 6099_u16, "tok".to_string());
    assert!(should_republish(Some(&last), 50000, 6100, "tok"));
}

#[test]
fn health_force_only_after_threshold() {
    assert!(!health_force_retunnel(2, 3));
    assert!(health_force_retunnel(3, 3));
}

#[test]
fn decide_retunnel_on_force_health_even_if_port_same() {
    assert_eq!(
        decide_tunnel_action(Some(6100), true, 6100, true),
        TunnelAction::Retunnel
    );
}

#[test]
fn decide_keep_when_healthy_same_port() {
    assert_eq!(
        decide_tunnel_action(Some(6100), true, 6100, false),
        TunnelAction::Keep
    );
}

#[test]
fn decide_retunnel_when_no_local() {
    assert_eq!(
        decide_tunnel_action(None, false, 6100, false),
        TunnelAction::Retunnel
    );
}
