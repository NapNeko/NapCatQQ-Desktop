//! 云崽这边跑 git 的共用部分：找 git、拼命令、几个源里挑最快的、clone 失败换源。
//!
//! 两个一定要带的设置：
//! - `GIT_TERMINAL_PROMPT=0`：仓库要登录（gitee 上有的插件改成私有了）时直接失败，别卡着等输入
//! - `safe.directory=*`：Windows 上实例目录在 ProgramData 下，属主和当前用户对不上时 git 会拒绝操作。
//!   我们自己的命令用 `-c`，云崽进程里它自己调的 git 走 `GIT_CONFIG_*` 环境变量（同属命令行级配置）

use std::time::Duration;

use ncd_host::{Host, HostCommand, HostPath, Os};

/// ls-remote 挑源时每个源最多等多久
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
/// clone / fetch 上限：带资源的大插件（喵喵）浅克隆也要几分钟
pub const GIT_LONG_TIMEOUT: Duration = Duration::from_secs(20 * 60);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GitTool {
    /// 命令名或绝对路径（托管的 MinGit 是绝对路径，PATH 上的就是 `git`）
    pub program: String,
    /// 托管 git 的 `cmd/` 目录；进程 PATH 要带上它，云崽自己才调得到 git
    pub dir: Option<HostPath>,
}

impl GitTool {
    /// 托管的优先，其次 PATH；都没有给一句能照着做的话
    pub async fn resolve(host: &dyn Host, managed: Option<&HostPath>) -> Result<Self, String> {
        if let Some(bin) = managed {
            if host.exists(bin).await.unwrap_or(false) {
                return Ok(Self {
                    program: bin.as_posix().to_string(),
                    dir: bin.parent(),
                });
            }
        }
        if host.command_exists("git").await {
            return Ok(Self {
                program: "git".to_string(),
                dir: None,
            });
        }
        Err(match host.os() {
            Os::Windows => "没找到 git：先在组件页装好「Git」再装云崽".to_string(),
            _ => "没找到 git：先在组件页给这台主机装「Git」（或 apt install git）".to_string(),
        })
    }

    pub fn command<'a>(&self, args: impl IntoIterator<Item = &'a str>) -> HostCommand {
        HostCommand::new(&self.program)
            .arg("-c")
            .arg("safe.directory=*")
            .args(args)
            .env("GIT_TERMINAL_PROMPT", "0")
    }

    /// 并发 `ls-remote` 几个源，返回最先应答的那个；全都不通时带上最后一个错误
    pub async fn pick_fastest(&self, host: &dyn Host, urls: &[String]) -> Result<String, String> {
        if urls.is_empty() {
            return Err("没有可用的源".into());
        }
        let probes = urls.iter().map(|url| {
            let cmd = self
                .command(["ls-remote", "--heads", url.as_str()])
                .timeout(PROBE_TIMEOUT);
            let url = url.clone();
            Box::pin(async move {
                match host.run_to_string(cmd).await {
                    Ok(out) if out.success() => Ok(url),
                    Ok(out) => Err(format!("{url}: {}", last_line(&out.stderr))),
                    Err(e) => Err(format!("{url}: {e}")),
                }
            })
        });
        futures_util::future::select_ok(probes)
            .await
            .map(|(url, _rest)| url)
            .map_err(|e| format!("几个源都连不上（{e}）"))
    }
}

/// 云崽进程要继承的 git 环境（它自己的 #更新 / #安装插件 也是调 git）
pub fn git_env() -> Vec<(&'static str, &'static str)> {
    vec![
        ("GIT_TERMINAL_PROMPT", "0"),
        ("GIT_CONFIG_COUNT", "1"),
        ("GIT_CONFIG_KEY_0", "safe.directory"),
        ("GIT_CONFIG_VALUE_0", "*"),
    ]
}

/// GitHub 地址多给几个加速前缀的版本（git 走 HTTPS 智能协议，这些代理都能转）；别的主机原样
pub fn clone_candidates(urls: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |u: String| {
        if !out.contains(&u) {
            out.push(u);
        }
    };
    for url in urls {
        push(url.clone());
    }
    for url in urls {
        if url.starts_with("https://github.com/") {
            for mirrored in ncd_network::build_mirror_urls(url, None).into_iter().skip(1) {
                push(mirrored);
            }
        }
    }
    out
}

pub fn last_line(s: &str) -> &str {
    s.trim().lines().last().unwrap_or_default().trim()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_forces_non_interactive_and_safe_directory() {
        let git = GitTool {
            program: "git".into(),
            dir: None,
        };
        let cmd = git.command(["clone", "--depth", "1", "u", "d"]);
        assert_eq!(cmd.program, "git");
        assert_eq!(
            cmd.args,
            vec!["-c", "safe.directory=*", "clone", "--depth", "1", "u", "d"]
        );
        assert_eq!(
            cmd.environment.get("GIT_TERMINAL_PROMPT").map(String::as_str),
            Some("0")
        );
    }

    #[test]
    fn github_urls_get_mirror_fallbacks_after_all_originals() {
        let got = clone_candidates(&[
            "https://github.com/yoimiya-kokomi/miao-plugin".to_string(),
            "https://gitcode.com/TimeRainStarSky/miao-plugin.git".to_string(),
        ]);
        assert_eq!(got[0], "https://github.com/yoimiya-kokomi/miao-plugin");
        assert_eq!(got[1], "https://gitcode.com/TimeRainStarSky/miao-plugin.git");
        assert!(got[2..].iter().all(|u| u.ends_with("/https://github.com/yoimiya-kokomi/miao-plugin")));
        let gitee_only = clone_candidates(&["https://gitee.com/a/b".to_string()]);
        assert_eq!(gitee_only, vec!["https://gitee.com/a/b".to_string()]);
    }

    #[test]
    fn process_env_carries_safe_directory_as_command_line_config() {
        let env = git_env();
        assert!(env.contains(&("GIT_CONFIG_KEY_0", "safe.directory")));
        assert!(env.contains(&("GIT_CONFIG_COUNT", "1")));
    }
}
