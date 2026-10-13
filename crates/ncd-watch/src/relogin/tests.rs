use super::*;
use wiremock::{
    Mock, MockServer, ResponseTemplate,
    matchers::{method, path},
};

#[test]
fn only_definitive_business_success_counts_as_delivery() {
    fn raw(value: Value) -> Result<ncd_onebot::client::RawCall, ClientError> {
        Ok(ncd_onebot::client::RawCall {
            text: value.to_string(),
            value,
        })
    }
    assert_eq!(
        delivery(raw(
            json!({"status":"ok","retcode":0,"data":{"message_id":123}})
        )),
        Delivery::Sent {
            message_id: "123".into()
        }
    );
    assert_eq!(
        delivery(raw(json!({"status":"ok","retcode":0}))),
        Delivery::Unknown
    );
    assert_eq!(
        delivery(raw(json!({"status":"failed","retcode":100}))),
        Delivery::Rejected
    );
    assert_eq!(delivery(Err(ClientError::Timeout)), Delivery::Unknown);
    assert_eq!(delivery(Err(ClientError::NotSent)), Delivery::Rejected);
}

#[test]
fn qr_payload_never_fetches_arbitrary_network_addresses() {
    assert!(
        qr_image("https://txz.qq.com/p?k=test")
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    for url in [
        "http://127.0.0.1/admin",
        "file:///etc/passwd",
        "https://txz.qq.com.evil.test/x",
        "https://user@txz.qq.com/p",
    ] {
        assert!(qr_image(url).is_err());
    }
}

#[test]
fn wrong_account_and_loading_are_unknown_not_offline() {
    let process = |value| serde_json::from_value::<webui::Process>(value).unwrap();
    assert_eq!(
        webui::account_state(
            &process(json!({"pid":1,"path":"/qq","uin":"2","status":"online","loggedIn":true})),
            "1"
        ),
        AccountState::Unknown
    );
    assert_eq!(
        webui::account_state(
            &process(json!({"pid":1,"path":"/qq","uin":"","status":"loading"})),
            "1"
        ),
        AccountState::Unknown
    );
    assert_eq!(
        webui::account_state(
            &process(
                json!({"pid":1,"path":"/qq","uin":"1","status":"online","loggedIn":true,"connected":true})
            ),
            "1"
        ),
        AccountState::Online
    );
    assert_eq!(
        webui::account_state(
            &process(
                json!({"pid":1,"path":"/qq","uin":"1","status":"disconnected","loggedIn":true,"connected":false})
            ),
            "1"
        ),
        AccountState::Offline
    );
}

#[tokio::test]
async fn shared_sl_daemon_logs_in_once_for_multiple_bots() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token":"test"})))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/processes"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"list":[]})))
        .expect(2)
        .mount(&server)
        .await;
    let mut client = webui::WebUi::new().unwrap();
    for bot in ["1", "2"] {
        client
            .request(
                bot,
                server.address().port(),
                "secret",
                true,
                "/api/processes",
            )
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn http_200_auth_failure_is_backed_off_across_bots() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/auth/login"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"code":-1,"message":"Unauthorized"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let mut client = webui::WebUi::new().unwrap();
    for bot in ["1", "1", "2"] {
        assert!(
            client
                .request(
                    bot,
                    server.address().port(),
                    "secret",
                    false,
                    "/api/QQLogin/CheckLoginStatus"
                )
                .await
                .is_err()
        );
    }
}

#[tokio::test]
async fn malformed_or_large_webui_response_does_not_become_offline() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/login"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"token":"test"})))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/processes"))
        .respond_with(ResponseTemplate::new(200).set_body_string("x".repeat(1024 * 1024 + 1)))
        .mount(&server)
        .await;
    let mut client = webui::WebUi::new().unwrap();
    assert!(
        client
            .request(
                "1",
                server.address().port(),
                "secret",
                true,
                "/api/processes"
            )
            .await
            .is_err()
    );
}

#[tokio::test]
async fn nc_phone_confirmation_never_invokes_refresh_endpoint() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/auth/login"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"code":0,"data":{"Credential":"test"}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST")).and(path("/api/QQLogin/CheckLoginStatus"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({"code":0,"data":{"isLogin":false,"loginPhase":"qrcode_scanned","qrcodeurl":"https://txz.qq.com/p?k=stale"}}))).mount(&server).await;
    Mock::given(method("POST"))
        .and(path("/api/QQLogin/GetQQLoginInfo"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({"code":0,"data":{"uin":"10001","online":false}})),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/QQLogin/RefreshQRcode"))
        .respond_with(ResponseTemplate::new(500))
        .expect(0)
        .mount(&server)
        .await;
    let dir = tempfile::tempdir().unwrap();
    let paths = WatchPaths::from_root(dir.path());
    let mut config = NotifyConfig::example();
    config.bots[0].deployment = "native".into();
    config.bots[0].process_match = None;
    config.bots[0].pid_file = None;
    config.onebot.enabled = true;
    config.onebot.relogin.settings.enabled = true;
    config.onebot.relogin.settings.administrator_ids = vec![99];
    config.onebot.relogin.login_targets = vec![WatchLoginTarget {
        bot_id: "10001".into(),
        webui_port: server.address().port(),
        webui_secret: "test".into(),
        qr_enabled: true,
        ..Default::default()
    }];
    std::fs::create_dir_all(&paths.config_dir).unwrap();
    std::fs::write(&paths.notify_json, serde_json::to_vec(&config).unwrap()).unwrap();
    let port = WatchPort {
        config_path: paths.notify_json,
        config,
        webui: Mutex::new(webui::WebUi::new().unwrap()),
        connections: HashMap::new(),
        processes: Mutex::new(HashMap::new()),
    };
    let reply = port.qr("10001", true).await.unwrap();
    assert_eq!(reply.snapshot.state, SnowLumaQrLoginState::Scanned);
    assert!(reply.image().is_none());
}

#[tokio::test]
async fn absent_process_is_offline_but_invalid_probe_stays_unknown() {
    let dir = tempfile::tempdir().unwrap();
    let paths = WatchPaths::from_root(dir.path());
    let mut config = NotifyConfig::example();
    config.bots[0].deployment = "native".into();
    config.bots[0].process_match = None;
    let pid_file = dir.path().join("qq.pid");
    config.bots[0].pid_file = Some(pid_file.to_string_lossy().into_owned());
    let port = WatchPort {
        config_path: paths.notify_json,
        config,
        webui: Mutex::new(webui::WebUi::new().unwrap()),
        connections: HashMap::new(),
        processes: Mutex::new(HashMap::new()),
    };
    assert_eq!(
        port.observe("10001").await.unwrap().state,
        AccountState::Offline
    );
    std::fs::write(pid_file, b"invalid").unwrap();
    assert_eq!(
        port.observe("10001").await.unwrap().state,
        AccountState::Unknown
    );
}
