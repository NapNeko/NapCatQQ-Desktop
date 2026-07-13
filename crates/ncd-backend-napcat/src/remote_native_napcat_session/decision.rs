//! 隧道/发布决策（纯函数，单测友好）

/// 上次成功对外发布的 (local, remote, token)
pub(crate) type PublishedWebui = (u16, u16, String);

/// 是否需要按新发现的远端 WebUI 口重建隧道
pub(crate) fn should_retunnel(
    slot_remote: Option<u16>,
    has_local_port: bool,
    discovered_remote: u16,
) -> bool {
    !has_local_port || slot_remote != Some(discovered_remote)
}

/// 是否需要再发 NapCatWebuiAvailable(本机口 / 远端口 / token 任一变化)
pub(crate) fn should_republish(
    last: Option<&PublishedWebui>,
    local_port: u16,
    remote_port: u16,
    token: &str,
) -> bool {
    !matches!(
        last,
        Some((lp, rp, t)) if *lp == local_port && *rp == remote_port && t == token
    )
}

/// 健康失败计数:连续失败达到阈值才强制 retunnel
pub(crate) fn health_force_retunnel(consecutive_fails: u32, threshold: u32) -> bool {
    consecutive_fails >= threshold
}

/// 本 tick 对隧道要做的动作(纯决策,便于单测)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TunnelAction {
    Keep,
    Retunnel,
}

pub(crate) fn decide_tunnel_action(
    slot_remote: Option<u16>,
    has_local_port: bool,
    discovered_remote: u16,
    force_health: bool,
) -> TunnelAction {
    if force_health || should_retunnel(slot_remote, has_local_port, discovered_remote) {
        TunnelAction::Retunnel
    } else {
        TunnelAction::Keep
    }
}
