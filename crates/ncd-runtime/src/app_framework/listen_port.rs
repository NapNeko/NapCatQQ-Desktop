//! 应用实例监听口：未指定则随机高位，避开同机已登记端口；本机再探一次能否 bind。

use rand::Rng;

/// 避开特权口与常见框架默认（7777 / 8080），也避开系统临时端口段。
pub const APP_LISTEN_PORT_MIN: u16 = 20_000;
pub const APP_LISTEN_PORT_MAX: u16 = 49_151;
const AUTO_TRIES: u32 = 48;

pub fn allocate_listen_port(
    requested: Option<u16>,
    taken: &[u16],
    probe_local: bool,
) -> Result<u16, String> {
    allocate_listen_port_in(
        requested,
        taken,
        probe_local,
        APP_LISTEN_PORT_MIN,
        APP_LISTEN_PORT_MAX,
        &mut rand::thread_rng(),
    )
}

fn allocate_listen_port_in<R: Rng + ?Sized>(
    requested: Option<u16>,
    taken: &[u16],
    probe_local: bool,
    min: u16,
    max: u16,
    rng: &mut R,
) -> Result<u16, String> {
    if let Some(port) = requested {
        return accept_port(port, taken, probe_local);
    }
    if min > max {
        return Err("无法分配空闲监听端口，请手动指定".into());
    }
    let span = u32::from(max - min) + 1;
    for _ in 0..AUTO_TRIES {
        let port = min + (rng.gen_range(0..span) as u16);
        if accept_port(port, taken, probe_local).is_ok() {
            return Ok(port);
        }
    }
    Err("无法分配空闲监听端口，请手动指定".into())
}

fn accept_port(port: u16, taken: &[u16], probe_local: bool) -> Result<u16, String> {
    if port == 0 {
        return Err("端口不能为 0".into());
    }
    if taken.contains(&port) {
        return Err(format!("该主机上已有应用实例占用端口 {port}"));
    }
    if probe_local && !local_port_free(port) {
        return Err(format!("本机端口 {port} 已被其它程序占用"));
    }
    Ok(port)
}

pub fn local_port_free(port: u16) -> bool {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    #[test]
    fn requested_zero_is_rejected() {
        let err = allocate_listen_port(Some(0), &[], false).unwrap_err();
        assert!(err.contains("0"), "{err}");
    }

    #[test]
    fn requested_taken_is_rejected() {
        let err = allocate_listen_port(Some(8080), &[8080], false).unwrap_err();
        assert!(err.contains("8080"), "{err}");
    }

    #[test]
    fn requested_free_is_kept() {
        assert_eq!(allocate_listen_port(Some(32100), &[], false).unwrap(), 32100);
    }

    #[test]
    fn auto_picks_the_only_free_port_in_a_tiny_range() {
        let mut rng = StdRng::seed_from_u64(1);
        let port = allocate_listen_port_in(None, &[20_000], false, 20_000, 20_001, &mut rng).unwrap();
        assert_eq!(port, 20_001);
    }

    #[test]
    fn auto_fails_when_range_is_exhausted() {
        let mut rng = StdRng::seed_from_u64(1);
        let err = allocate_listen_port_in(None, &[7], false, 7, 7, &mut rng).unwrap_err();
        assert!(err.contains("无法分配"), "{err}");
    }
}
