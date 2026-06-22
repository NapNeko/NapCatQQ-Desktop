//! HostCommand:跨平台命令构建器
//!
//! 设计要点:
//! - 不用 std::process::Command,因为它已经绑定本机进程模型,远端 SSH 用不了
//! - 命令的 program 与 args 单独存,落地由 [HostShell](crate::HostShell) 做
//!   shell escape(本地 tokio::process 直接传 args list,远端 SSH 拼成 shell 字符串)
//! - 环境变量用 BTreeMap 保证序列化字节稳定(对齐字节级 round-trip 红线)
//! - working_dir 用 [HostPath](crate::HostPath) 而非 PathBuf,跨平台

use std::collections::BTreeMap;
use std::time::Duration;

use crate::path::HostPath;

/// 默认命令等待上限短命令避免无限挂起,长生命周期进程要显式选择
/// [HostProcessWaitPolicy::NoTimeout]
pub const DEFAULT_COMMAND_TIMEOUT: Duration = Duration::from_secs(300);

/// HostProcess::wait 的等待策略
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostProcessWaitPolicy {
    /// 使用命令超时;未设置时使用 [DEFAULT_COMMAND_TIMEOUT]
    Default,
    /// HostProcess::wait 使用指定上限
    Timeout(Duration),
    /// HostProcess::wait 不设置超时,适合 NapCat / SnowLuma 这类长生命周期进程
    NoTimeout,
}

impl HostProcessWaitPolicy {
    /// 解析成 tokio::time::timeout 可消费的上限
    pub fn resolve_timeout(self, command_timeout: Option<Duration>) -> Option<Duration> {
        match self {
            Self::Default => Some(command_timeout.unwrap_or(DEFAULT_COMMAND_TIMEOUT)),
            Self::Timeout(timeout) => Some(timeout),
            Self::NoTimeout => None,
        }
    }
}

/// 跨平台命令描述
///
/// 使用建议:
/// let cmd = HostCommand::new("git")
///     .arg("clone")
///     .arg("--depth=1")
///     .arg(repo_url)
///     .working_dir(workspace_root.clone())
///     .env("GIT_TERMINAL_PROMPT", "0")
///     .timeout(Duration::from_secs(60));
/// host.spawn(cmd).await?
#[derive(Debug, Clone)]
pub struct HostCommand {
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: Option<HostPath>,
    pub environment: BTreeMap<String, String>,
    pub timeout: Option<Duration>,
    pub wait_policy: HostProcessWaitPolicy,
    /// 是否需要提权运行(LocalWindows = UAC,Linux = sudo)
    pub elevated: bool,
    /// stdin 输入(如 echo "yes" | sudo apt install)None = 不传 stdin
    pub stdin: Option<Vec<u8>>,
}

impl HostCommand {
    /// 创建命令,只指定程序名(args 后续 .arg() 链式追加)
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            working_dir: None,
            environment: BTreeMap::new(),
            timeout: None,
            wait_policy: HostProcessWaitPolicy::Default,
            elevated: false,
            stdin: None,
        }
    }

    /// 追加单个参数(builder 模式)
    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    /// 批量追加参数
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args.extend(args.into_iter().map(Into::into));
        self
    }

    /// 设置工作目录
    pub fn working_dir(mut self, dir: HostPath) -> Self {
        self.working_dir = Some(dir);
        self
    }

    /// 设置单个环境变量
    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.environment.insert(key.into(), value.into());
        self
    }

    /// 批量设置环境变量
    pub fn envs<I, K, V>(mut self, vars: I) -> Self
    where
        I: IntoIterator<Item = (K, V)>,
        K: Into<String>,
        V: Into<String>,
    {
        for (k, v) in vars {
            self.environment.insert(k.into(), v.into());
        }
        self
    }

    /// 设置超时(None 表示使用 Host 默认上限)
    pub fn timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// 单独设置 HostProcess::wait 的超时,不影响 run_to_string / run_streaming
    pub fn wait_timeout(mut self, timeout: Duration) -> Self {
        self.wait_policy = HostProcessWaitPolicy::Timeout(timeout);
        self
    }

    /// 让 HostProcess::wait 不设置超时只应用于 spawn 返回的进程句柄
    pub fn no_wait_timeout(mut self) -> Self {
        self.wait_policy = HostProcessWaitPolicy::NoTimeout;
        self
    }

    /// 标记为长生命周期进程,等价于 [Self::no_wait_timeout]
    pub fn long_running(self) -> Self {
        self.no_wait_timeout()
    }

    /// 标记需要提权(具体提权机制由 Host 实装决定)
    pub fn elevated(mut self) -> Self {
        self.elevated = true;
        self
    }

    /// 设置 stdin 输入
    pub fn stdin(mut self, data: impl Into<Vec<u8>>) -> Self {
        self.stdin = Some(data.into());
        self
    }
}

/// 命令执行结果(Host::run_to_string / HostProcess::wait 返回值)
#[derive(Debug, Clone)]
pub struct CommandOutput {
    /// 退出码,None 表示进程被信号杀死
    pub exit_code: Option<i32>,
    /// stdout(全量收集,适用于短命令;长输出用 Host::spawn + 流式读)
    pub stdout: String,
    /// stderr(同上)
    pub stderr: String,
}

impl CommandOutput {
    /// 是否成功(exit_code == Some(0))
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_collects_args_in_order() {
        let cmd = HostCommand::new("git")
            .arg("clone")
            .arg("--depth=1")
            .arg("https://example.com/repo");
        assert_eq!(cmd.program, "git");
        assert_eq!(
            cmd.args,
            vec!["clone", "--depth=1", "https://example.com/repo"]
        );
    }

    #[test]
    fn env_vars_are_sorted_for_byte_stability() {
        // BTreeMap 保证序列化时 key 字典序,跨平台字节稳定
        let cmd = HostCommand::new("rustc")
            .env("ZZZ", "last")
            .env("AAA", "first")
            .env("MMM", "middle");
        let keys: Vec<_> = cmd.environment.keys().collect();
        assert_eq!(keys, vec!["AAA", "MMM", "ZZZ"]);
    }

    #[test]
    fn timeout_can_be_set_and_read() {
        let cmd = HostCommand::new("sleep")
            .arg("5")
            .timeout(Duration::from_secs(2));
        assert_eq!(cmd.timeout, Some(Duration::from_secs(2)));
    }

    #[test]
    fn wait_policy_defaults_to_command_timeout_or_host_default() {
        assert_eq!(
            HostProcessWaitPolicy::Default.resolve_timeout(Some(Duration::from_secs(2))),
            Some(Duration::from_secs(2))
        );
        assert_eq!(
            HostProcessWaitPolicy::Default.resolve_timeout(None),
            Some(DEFAULT_COMMAND_TIMEOUT)
        );
    }

    #[test]
    fn wait_policy_builder_can_override_process_wait_only() {
        let cmd = HostCommand::new("sleep")
            .timeout(Duration::from_secs(2))
            .wait_timeout(Duration::from_secs(30));
        assert_eq!(cmd.timeout, Some(Duration::from_secs(2)));
        assert_eq!(
            cmd.wait_policy.resolve_timeout(cmd.timeout),
            Some(Duration::from_secs(30))
        );
    }

    #[test]
    fn long_running_process_wait_has_no_timeout() {
        let cmd = HostCommand::new("napcat")
            .timeout(Duration::from_secs(2))
            .long_running();
        assert_eq!(cmd.timeout, Some(Duration::from_secs(2)));
        assert_eq!(cmd.wait_policy, HostProcessWaitPolicy::NoTimeout);
        assert_eq!(cmd.wait_policy.resolve_timeout(cmd.timeout), None);
    }

    #[test]
    fn no_wait_timeout_is_long_running_alias() {
        let cmd = HostCommand::new("snowluma").no_wait_timeout();
        assert_eq!(cmd.wait_policy, HostProcessWaitPolicy::NoTimeout);
    }

    #[test]
    fn elevated_flag_off_by_default() {
        assert!(!HostCommand::new("ls").elevated);
        assert!(
            HostCommand::new("apt-get")
                .arg("install")
                .elevated()
                .elevated
        );
    }

    #[test]
    fn working_dir_uses_host_path() {
        let cmd = HostCommand::new("ls").working_dir(HostPath::from_posix("/etc"));
        assert_eq!(cmd.working_dir.as_ref().unwrap().as_posix(), "/etc");
    }

    #[test]
    fn output_success_checks_exit_code() {
        let ok = CommandOutput {
            exit_code: Some(0),
            stdout: String::new(),
            stderr: String::new(),
        };
        let fail = CommandOutput {
            exit_code: Some(1),
            stdout: String::new(),
            stderr: String::new(),
        };
        let killed = CommandOutput {
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
        };
        assert!(ok.success());
        assert!(!fail.success());
        assert!(!killed.success());
    }
}
