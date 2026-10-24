use super::*;

#[tokio::test]
async fn failed_stop_restores_running_with_a_fresh_token_and_remains_refreshable() {
    let actor = BotActorHandle::spawn("stop-recovery");
    actor.request_start().await.unwrap();
    let running = actor.confirm_running().await.unwrap();
    let cancelled = actor.cancellation_token();
    let stopping = actor.request_stop().await.unwrap();
    assert!(cancelled.is_cancelled());
    let restored = actor
        .abort_stop(stopping.revision, true, "status unavailable".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(restored.state, BotActorState::Running);
    assert!(restored.token_generation > running.token_generation);
    assert!(!actor.cancellation_token().is_cancelled());
    assert_eq!(restored.last_error.as_deref(), Some("status unavailable"));
    let observed = actor
        .reconcile_observed(restored.revision, false)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(observed.state, BotActorState::Stopped);
}

#[tokio::test]
async fn an_old_failed_stop_cannot_overwrite_a_new_stop_generation() {
    let actor = BotActorHandle::spawn("stale-stop-recovery");
    actor.request_start().await.unwrap();
    actor.confirm_running().await.unwrap();
    let old = actor.request_stop().await.unwrap();
    actor.confirm_stopped().await.unwrap();
    actor.request_start().await.unwrap();
    actor.confirm_running().await.unwrap();
    let current = actor.request_stop().await.unwrap();
    assert!(
        actor
            .confirm_stopped_revision(old.revision)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        actor
            .abort_stop(old.revision, true, "late failure".into())
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(actor.snapshot(), current);
}

#[tokio::test]
async fn stopping_an_unconfirmed_start_never_claims_the_runtime_has_exited() {
    let actor = BotActorHandle::spawn("starting-stop-recovery");
    actor.request_start().await.unwrap();
    let stopping = actor.request_stop().await.unwrap();
    let failed = actor
        .abort_stop(stopping.revision, false, "stop timeout".into())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(failed.state, BotActorState::Crashed);
    let observed = actor
        .reconcile_observed(failed.revision, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(observed.state, BotActorState::Running);
}

#[tokio::test]
async fn old_start_results_cannot_commit_or_fail_a_replacement_generation() {
    let actor = BotActorHandle::spawn("stale-start");
    let old = actor.request_start().await.unwrap();
    let stopping = actor.request_stop().await.unwrap();
    assert!(
        actor
            .confirm_running_revision(old.revision)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        actor
            .abort_start(old.revision, Some("old failure".into()))
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(actor.snapshot(), stopping);
    actor
        .confirm_stopped_revision(stopping.revision)
        .await
        .unwrap()
        .unwrap();
    let current = actor.request_start().await.unwrap();
    assert!(
        actor
            .confirm_running_revision(old.revision)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        actor
            .abort_start(old.revision, None)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(actor.snapshot(), current);
    let running = actor
        .confirm_running_revision(current.revision)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(running.state, BotActorState::Running);
}

#[tokio::test]
async fn cancelled_commit_before_ack_settles_only_its_own_running_generation() {
    let actor = BotActorHandle::spawn("commit-before-ack");
    let starting = actor.request_start().await.unwrap();
    let running = actor
        .confirm_running_revision(starting.revision)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(running.revision, starting.revision + 1);
    let stopped = actor
        .settle_unacknowledged_start(starting.revision, starting.token_generation, true)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stopped.state, BotActorState::Stopped);
    let replacement = actor.request_start().await.unwrap();
    let replacement = actor
        .confirm_running_revision(replacement.revision)
        .await
        .unwrap()
        .unwrap();
    assert!(
        actor
            .settle_unacknowledged_start(starting.revision, starting.token_generation, true)
            .await
            .unwrap()
            .is_none()
    );
    assert_eq!(actor.snapshot(), replacement);
}
