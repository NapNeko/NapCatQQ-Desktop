//! 只读请求被上层取消后，不让下一请求继续复用可能卡住的 SFTP 会话。

use russh_sftp::client::SftpSession;
use std::sync::Arc;
use tokio::sync::Mutex;

type SessionCache = Arc<Mutex<Option<Arc<SftpSession>>>>;

pub(in crate::remote) struct SftpReadLease {
    cache: SessionCache,
    session: Arc<SftpSession>,
    completed: bool,
}

impl SftpReadLease {
    pub(super) fn new(cache: SessionCache, session: Arc<SftpSession>) -> Self {
        Self {
            cache,
            session,
            completed: false,
        }
    }

    pub(crate) fn complete(&mut self) {
        self.completed = true;
    }
}

fn remove_matching(cached: &mut Option<Arc<SftpSession>>, session: &Arc<SftpSession>) {
    if cached
        .as_ref()
        .is_some_and(|current| Arc::ptr_eq(current, session))
    {
        *cached = None;
    }
}

impl Drop for SftpReadLease {
    fn drop(&mut self) {
        if self.completed {
            return;
        }
        // 只退役缓存；正在写文件的使用者仍持有旧会话，最后一个使用者退出时再关闭。
        if let Ok(mut cached) = self.cache.try_lock() {
            remove_matching(&mut cached, &self.session);
        } else {
            let cache = Arc::clone(&self.cache);
            let session = Arc::clone(&self.session);
            tokio::spawn(async move {
                remove_matching(&mut *cache.lock().await, &session);
            });
        }
    }
}
