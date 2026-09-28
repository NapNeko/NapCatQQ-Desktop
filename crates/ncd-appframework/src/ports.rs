//! 应用端挑口、查口共用的占用探测：本机直接 bind 试，远端一次读 `/proc/net/tcp{,6}` 拿到全部监听口。
//! 远端不逐口起 python 去 bind：几十个候选口就是几十次 SSH 往返，机器上没有 python 时还会一律当成空闲

use std::time::Duration;

use ncd_host::{Host, HostCommand, Locality};

/// 本机口能不能 bind。只试回环：bind 0.0.0.0 会真的 listen，Windows 上可能弹防火墙询问
pub fn local_port_free(port: u16) -> bool {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).is_ok()
}

/// 远端 Linux 上正在监听的 TCP 口，不依赖 ss / netstat 装没装。
/// 命令跑不通或什么也没读到给 None：那是不知道，不是没人占，兜底由调用方定
pub async fn remote_listening_ports(host: &dyn Host) -> Option<Vec<u16>> {
    let cmd = HostCommand::new("sh")
        .arg("-c")
        .arg("cat /proc/net/tcp /proc/net/tcp6 2>/dev/null")
        .timeout(Duration::from_secs(15));
    match host.run_to_string(cmd).await {
        Ok(out) if !out.stdout.trim().is_empty() => Some(parse_proc_net_listen(&out.stdout)),
        Ok(_) => None,
        Err(e) => {
            tracing::debug!(error = %e, "read remote listening ports");
            None
        }
    }
}

/// 每行 `sl local_address rem_address st …`，本地地址是 `十六进制 IP:十六进制口`，状态 `0A` 是 LISTEN
pub fn parse_proc_net_listen(text: &str) -> Vec<u16> {
    let mut ports: Vec<u16> = text
        .lines()
        .filter_map(|line| {
            let mut cols = line.split_whitespace();
            let _slot = cols.next()?;
            let local = cols.next()?;
            let _remote = cols.next()?;
            if cols.next()? != "0A" {
                return None;
            }
            u16::from_str_radix(local.rsplit_once(':')?.1, 16).ok()
        })
        .filter(|p| *p != 0)
        .collect();
    ports.sort_unstable();
    ports.dedup();
    ports
}

/// 挑口时一台主机的占用情况：远端的监听表只读一次，之后逐口比对
pub enum PortUsage {
    Local,
    Remote(Vec<u16>),
}

impl PortUsage {
    /// 远端读不到监听表时为 None
    pub async fn probe(host: &dyn Host) -> Option<Self> {
        if host.locality() == Locality::Local {
            return Some(Self::Local);
        }
        remote_listening_ports(host).await.map(Self::Remote)
    }

    pub fn is_free(&self, port: u16) -> bool {
        match self {
            Self::Local => local_port_free(port),
            Self::Remote(busy) => !busy.contains(&port),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_net_listen_keeps_only_listening_ports() {
        let text = "  sl  local_address rem_address   st tx_queue rx_queue tr tm->when retrnsmt   uid  timeout inode\n\
   0: 0100007F:1F41 00000000:0000 0A 00000000:00000000 00:00000000 00000000  1000        0 1 1 0\n\
   1: 00000000:0016 00000000:0000 0A 00000000:00000000 00:00000000 00000000     0        0 2 1 0\n\
   2: 0100007F:1F41 0100007F:D431 01 00000000:00000000 00:00000000 00000000  1000        0 3 1 0\n\
  sl  local_address                         remote_address                        st\n\
   0: 00000000000000000000000001000000:1F41 00000000000000000000000000000000:0000 0A 0 0 0\n\
   1: 00000000000000000000000000000000:1B59 00000000000000000000000000000000:0000 0A 0 0 0\n";
        assert_eq!(
            parse_proc_net_listen(text),
            vec![22, 7001, 8001],
            "去重、只留 LISTEN（0A）"
        );
        assert!(parse_proc_net_listen("").is_empty());
    }

    #[test]
    fn remote_usage_checks_against_the_listening_table() {
        let usage = PortUsage::Remote(vec![22, 6185]);
        assert!(!usage.is_free(6185));
        assert!(usage.is_free(6186));
    }

    #[test]
    fn local_usage_sees_a_bound_loopback_port() {
        let held = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = held.local_addr().unwrap().port();
        assert!(!PortUsage::Local.is_free(port));
    }
}
