//! 启动阶段只在已完成的副作用边界响应取消。

use std::future::Future;

use ncd_traits::runtime_backend::BotBackendError;
use tokio_util::sync::CancellationToken;

pub(super) fn check(cancel: Option<&CancellationToken>) -> Result<(), BotBackendError> {
    if cancel.is_some_and(CancellationToken::is_cancelled) {
        Err(BotBackendError::Io("SnowLuma 启动已取消".into()))
    } else {
        Ok(())
    }
}

pub(super) async fn read_only<T>(
    cancel: Option<&CancellationToken>,
    future: impl Future<Output = Result<T, BotBackendError>>,
) -> Result<T, BotBackendError> {
    if let Some(cancel) = cancel {
        tokio::select! {
            biased;
            _ = cancel.cancelled() => check(Some(cancel)).map(|_| unreachable!()),
            result = future => result,
        }
    } else {
        future.await
    }
}
