use std::sync::Arc;

use ncd_core::{
    AdvancedConfig, AutoRestartSchedule, BackendType, BotBasicConfig, BotConfig, BotConfigError,
    BotConfigRepo, ConfigStore, ConnectConfig, JsonTransaction, LocalBotConfigRepo,
    LocalConfigStore, RuntimeTarget, SecretStore, SecretStoreImpl,
};

fn bot_config(qq_id: u64, name: &str) -> BotConfig {
    BotConfig {
        bot: BotBasicConfig {
            name: name.to_string(),
            qq_id,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: BackendType::NapCat,
            snowluma_start_mode: None,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
    }
}

fn make_repo(
    root: &std::path::Path,
) -> (Arc<LocalConfigStore>, LocalBotConfigRepo<LocalConfigStore>) {
    let store = Arc::new(LocalConfigStore::new(root));
    let secrets: Arc<dyn SecretStore + Send + Sync> = Arc::new(
        SecretStoreImpl::new_with_force_fallback(root.join("secrets"), true),
    );
    let repo = LocalBotConfigRepo::new(Arc::clone(&store), secrets);
    (store, repo)
}

#[tokio::test]
async fn test_empty_repo_lists_empty() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo) = make_repo(temp.path());

    assert_eq!(repo.list().await.unwrap(), Vec::new());
}

#[tokio::test]
async fn test_upsert_then_list_returns_one() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo) = make_repo(temp.path());

    repo.upsert(bot_config(10001, "x")).await.unwrap();
    let bots = repo.list().await.unwrap();

    assert_eq!(bots.len(), 1);
    assert_eq!(bots[0].bot.qq_id, 10001);
    assert_eq!(bots[0].bot.name, "x");
}

#[tokio::test]
async fn test_upsert_same_qq_id_replaces() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo) = make_repo(temp.path());

    repo.upsert(bot_config(10001, "x")).await.unwrap();
    repo.upsert(bot_config(10001, "y")).await.unwrap();
    let bots = repo.list().await.unwrap();

    assert_eq!(bots.len(), 1);
    assert_eq!(bots[0].bot.name, "y");
}

#[tokio::test]
async fn test_count_works() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo) = make_repo(temp.path());

    repo.upsert(bot_config(10001, "x")).await.unwrap();
    repo.upsert(bot_config(10002, "y")).await.unwrap();

    assert_eq!(repo.count().await.unwrap(), 2);
}

#[tokio::test]
async fn test_delete_existing_returns_true_and_removes() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo) = make_repo(temp.path());

    repo.upsert(bot_config(10001, "x")).await.unwrap();

    assert!(repo.delete(10001).await.unwrap());
    assert_eq!(repo.list().await.unwrap(), Vec::new());
}

#[tokio::test]
async fn test_delete_missing_returns_false() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo) = make_repo(temp.path());

    repo.upsert(bot_config(10001, "x")).await.unwrap();

    assert!(!repo.delete(10002).await.unwrap());
    assert_eq!(repo.count().await.unwrap(), 1);
}

#[tokio::test]
async fn test_get_missing_returns_none() {
    let temp = tempfile::tempdir().unwrap();
    let (_, repo) = make_repo(temp.path());

    assert_eq!(repo.get(10001).await.unwrap(), None);
}

#[tokio::test]
async fn test_validate_failure_blocks_upsert() {
    let temp = tempfile::tempdir().unwrap();
    let (store, repo) = make_repo(temp.path());
    let config = BotConfig {
        bot: BotBasicConfig {
            name: "bad".to_string(),
            qq_id: 0,
            music_sign_url: String::new(),
            auto_restart_schedule: AutoRestartSchedule::default(),
            offline_auto_restart: false,
            runtime_target: RuntimeTarget::Local,
            backend_type: BackendType::NapCat,
            snowluma_start_mode: None,
        },
        connect: ConnectConfig::default(),
        advanced: AdvancedConfig::default(),
    };

    let error = repo.upsert(config).await.unwrap_err();

    assert!(matches!(error, BotConfigError::InvalidQqId(0)));
    assert!(!store.bot_config_path().exists());
}

#[tokio::test]
async fn test_list_rejects_duplicate_qq_ids() {
    let temp = tempfile::tempdir().unwrap();
    let (store, repo) = make_repo(temp.path());

    let payload = serde_json::json!({
        "info": {"configVersion": 999},
        "bots": [
            serde_json::to_value(bot_config(10001, "a")).unwrap(),
            serde_json::to_value(bot_config(10001, "b")).unwrap()
        ]
    });
    store
        .apply_transaction(JsonTransaction::new().write(store.bot_config_path(), payload))
        .unwrap();

    let error = repo.list().await.unwrap_err();

    assert!(matches!(error, BotConfigError::DuplicateQqId(10001)));
}

#[tokio::test]
async fn test_concurrent_upsert_does_not_lose_updates() {
    // 回归测试 codex M1 review 中的 Critical 问题：
    // 并发 upsert 走 read-modify-write，如果没有写锁就会互相覆盖。
    // 修复后 LocalBotConfigRepo 在 upsert/delete 上持 tokio::sync::Mutex 串行化，
    // 因此 8 路并发结束后 list 必须看到全部 8 个 Bot。
    let temp = tempfile::tempdir().unwrap();
    let (_, repo) = make_repo(temp.path());
    let repo = Arc::new(repo);

    let mut handles = Vec::new();
    for qq_id in 10001u64..10009 {
        let repo = Arc::clone(&repo);
        handles.push(tokio::spawn(async move {
            repo.upsert(bot_config(qq_id, &format!("bot-{qq_id}")))
                .await
        }));
    }
    for handle in handles {
        handle.await.unwrap().unwrap();
    }

    let bots = repo.list().await.unwrap();
    assert_eq!(bots.len(), 8);

    let mut qq_ids: Vec<u64> = bots.iter().map(|b| b.bot.qq_id).collect();
    qq_ids.sort();
    assert_eq!(qq_ids, (10001u64..10009).collect::<Vec<_>>());
}
