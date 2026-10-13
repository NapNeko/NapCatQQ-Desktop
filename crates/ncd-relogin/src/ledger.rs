// 发送前持久化投递意图，崩溃后保留结果未知的消息。
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountState {
    Online,
    Offline,
    Unknown,
    Stopped,
}

#[derive(Debug, Clone)]
pub struct Observation {
    pub bot_id: String,
    pub name: String,
    pub generation: String,
    pub state: AccountState,
    pub reset_generation: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub(crate) struct Account {
    #[serde(default)]
    pub reset_generation: String,
    pub seen_online: bool,
    pub generation: String,
    pub name: String,
    pub offline_since: Option<i64>,
    pub online_since: Option<i64>,
    pub last_notice: Option<i64>,
    pub incident: Option<Incident>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Incident {
    pub id: String,
    pub started_at: i64,
    pub ignored: bool,
    pub deliveries: BTreeMap<u64, Receipt>,
    pub participants: BTreeMap<u64, String>,
    pub refreshed_at: Option<i64>,
    #[serde(default)]
    pub group_deliveries: BTreeMap<u64, Receipt>,
    #[serde(default)]
    pub fallback_attempted: bool,
    #[serde(default)]
    pub retry_at: i64,
    #[serde(default)]
    pub refresh_replies: BTreeMap<u64, i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct Receipt {
    pub messenger: String,
    pub message_id: Option<String>,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub(crate) struct Ledger {
    pub v: u32,
    pub accounts: BTreeMap<String, Account>,
    pub commands: BTreeMap<String, i64>,
}

pub(crate) struct Store {
    path: PathBuf,
    _lock: std::fs::File,
}

impl Store {
    pub fn load(path: PathBuf) -> Result<(Self, Ledger), String> {
        let parent = path.parent().ok_or("缺少记录目录")?;
        std::fs::create_dir_all(parent).map_err(|_| "无法创建记录目录")?;
        let mut options = std::fs::OpenOptions::new();
        options.read(true).write(true).create(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600).custom_flags(libc::O_NOFOLLOW);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt;
            options.share_mode(0);
        }
        let lock = options
            .open(path.with_extension("lock"))
            .map_err(|_| "重登服务已运行或记录目录不可用")?;
        #[cfg(unix)]
        {
            use std::os::fd::AsRawFd;
            // 文件在 Store 生命周期内保持打开；进程退出由内核释放锁。
            if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
                return Err("重登服务已运行".into());
            }
        }
        let ledger = if path.exists() {
            let meta = std::fs::metadata(&path).map_err(|_| "无法读取重登记录")?;
            if meta.len() > 2 * 1024 * 1024 {
                return Err("重登记录过大".into());
            }
            let bytes = std::fs::read(&path).map_err(|_| "无法读取重登记录")?;
            let ledger: Ledger = serde_json::from_slice(&bytes)
                .map_err(|_| "重登记录损坏，已暂停通知以避免重复投递")?;
            if ledger.v != 1 || ledger.accounts.len() > 256 || ledger.commands.len() > 2048 {
                return Err("不支持的重登记录".into());
            }
            ledger
        } else {
            Ledger {
                v: 1,
                ..Default::default()
            }
        };
        Ok((Self { path, _lock: lock }, ledger))
    }

    pub fn save(&self, ledger: &Ledger) -> Result<(), String> {
        let bytes = serde_json::to_vec(ledger).map_err(|_| "无法编码重登记录")?;
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("重登记录过大，已暂停投递".into());
        }
        atomic_write(&self.path, &bytes).map_err(|_| "重登记录写入失败，已暂停投递".into())
    }
}

pub fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("missing parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary = parent.join(format!(".relogin-{}.tmp", uuid::Uuid::new_v4()));
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let result = (|| {
        let mut file = options.open(&temporary)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temporary, path)?;
        #[cfg(unix)]
        std::fs::File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&temporary);
    }
    result
}
