use super::*;
use std::future::{Future, poll_fn};
use std::task::Poll;

#[tokio::test]
async fn cancelled_bootstrap_wait_never_enters_remote_preparation() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    let _guard = daemon.stack_bootstrap.lock().await;
    let bot = BotId::new("10001");
    let cancel = CancellationToken::new();
    let start = daemon.ensure_running_for_bot_cancellable(&bot, None, false, None, &cancel);
    tokio::pin!(start);
    poll_fn(|context| {
        assert!(start.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), start)
            .await
            .expect("cancellation must not wait for the held bootstrap guard")
            .is_err()
    );
    assert!(host.commands.lock().unwrap().is_empty());
    assert_eq!(host.reads.load(Ordering::SeqCst), 0);
    assert_eq!(host.starts.load(Ordering::SeqCst), 0);
    assert_eq!(daemon.leases.lock().await.count(), 0);
}

#[tokio::test]
async fn abandoning_cancelled_start_preserves_the_other_bot_and_shared_environment() {
    let host = Arc::new(RecoveryHost::new(true, "running", false));
    let daemon = daemon(Arc::clone(&host), Arc::new(BroadcastEventBus::default()));
    let running = BotId::new("10001");
    let cancelled = BotId::new("10002");
    {
        let mut leases = daemon.leases.lock().await;
        leases.acquire(Some(&running));
        leases.acquire_start(Some(&cancelled));
    }
    *daemon.tunnel_remote_ports.lock().await = Some((13105, 6081));
    let _guard = daemon.stack_bootstrap.lock().await;
    let cancel = CancellationToken::new();
    let start = daemon.ensure_running_for_bot_cancellable(&cancelled, None, false, None, &cancel);
    tokio::pin!(start);
    poll_fn(|context| {
        assert!(start.as_mut().poll(context).is_pending());
        Poll::Ready(())
    })
    .await;
    cancel.cancel();
    assert!(
        tokio::time::timeout(Duration::from_secs(1), start)
            .await
            .expect("cancellation must release its wait independently of another start")
            .is_err()
    );
    let original_host: Arc<dyn Host> = host.clone();
    daemon
        .abandon_start_on_host(&original_host, &cancelled)
        .await;
    let leases = daemon.leases.lock().await;
    assert_eq!(leases.count(), 1);
    assert!(leases.bots.contains(&running));
    assert!(!leases.bots.contains(&cancelled));
    assert!(leases.pending_bots.is_empty());
    drop(leases);
    assert_eq!(
        *daemon.tunnel_remote_ports.lock().await,
        Some((13105, 6081))
    );
    assert!(host.node_running.load(Ordering::SeqCst));
    assert!(host.commands.lock().unwrap().is_empty());
    assert_eq!(host.starts.load(Ordering::SeqCst), 0);
}
