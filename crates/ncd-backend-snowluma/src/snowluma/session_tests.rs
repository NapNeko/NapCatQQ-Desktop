    use super::*;

    #[test]
    fn session_path_appends_session_json() {
        let root = PathBuf::from("/tmp/snowluma");
        let path = session_path(&root);
        assert_eq!(path, PathBuf::from("/tmp/snowluma").join("session.json"));
    }

    #[test]
    fn generate_strong_password_min_length() {
        // len 小于 10 时被钳到 10
        let pwd = generate_strong_password(4);
        assert!(pwd.len() >= MIN_PASSWORD_LEN);

        // 大于下限时按指定长度返回
        let pwd = generate_strong_password(DEFAULT_PASSWORD_LEN);
        assert_eq!(pwd.len(), DEFAULT_PASSWORD_LEN);

        // 4 类字符各至少 1 个;不含空格
        assert!(pwd.bytes().any(|b| UPPERCASE.contains(&b)));
        assert!(pwd.bytes().any(|b| LOWERCASE.contains(&b)));
        assert!(pwd.bytes().any(|b| DIGITS.contains(&b)));
        assert!(pwd.bytes().any(|b| SPECIALS.contains(&b)));
        assert!(!pwd.contains(' '));
    }

    #[test]
    fn build_webui_json_payload_has_5_fields() {
        let payload = build_webui_json_payload("hello-world-1!", false).expect("build payload");
        assert_eq!(payload.len(), 5);
        for key in [
            "passwordHash",
            "passwordSalt",
            "mustChangePassword",
            "generatedAt",
            "updatedAt",
        ] {
            assert!(payload.contains_key(key), "missing key: {key}");
        }

        // hash / salt 必须是 hex(对应字节数 = 64 / 16 → hex 长度 = 128 / 32)
        let hash = payload
            .get("passwordHash")
            .and_then(|v| v.as_str())
            .expect("hash str");
        assert_eq!(hash.len(), SCRYPT_DKLEN * 2);
        assert!(hash.bytes().all(|b| b.is_ascii_hexdigit()));

        let salt = payload
            .get("passwordSalt")
            .and_then(|v| v.as_str())
            .expect("salt str");
        assert_eq!(salt.len(), SCRYPT_SALT_BYTES * 2);
        assert!(salt.bytes().all(|b| b.is_ascii_hexdigit()));

        assert_eq!(
            payload.get("mustChangePassword"),
            Some(&serde_json::Value::Bool(false))
        );
    }

    // ------------------------------------------------------------------------
    // 追加测试:强密码 100 次抽样 / 字节级 round-trip / scrypt 行为 /
    // ISO 8601 格式锁 / 文件级幂等 + 时间戳推进 /
    // render_daemon_globals 优先级
    // ------------------------------------------------------------------------

    /// 100 次随机生成,每次都满足"长度 ≥ 10,含 4 类字符,不含空格"
    #[test]
    fn generate_strong_password_includes_all_classes_100x() {
        for i in 0..100 {
            let pwd = generate_strong_password(MIN_PASSWORD_LEN);
            assert!(
                pwd.len() >= MIN_PASSWORD_LEN,
                "iter {i}: length {} < min {}",
                pwd.len(),
                MIN_PASSWORD_LEN
            );
            assert!(
                pwd.bytes().any(|b| UPPERCASE.contains(&b)),
                "iter {i}: missing uppercase in {pwd:?}"
            );
            assert!(
                pwd.bytes().any(|b| LOWERCASE.contains(&b)),
                "iter {i}: missing lowercase in {pwd:?}"
            );
            assert!(
                pwd.bytes().any(|b| DIGITS.contains(&b)),
                "iter {i}: missing digit in {pwd:?}"
            );
            assert!(
                pwd.bytes().any(|b| SPECIALS.contains(&b)),
                "iter {i}: missing special in {pwd:?}"
            );
            assert!(!pwd.contains(' '), "iter {i}: contains space in {pwd:?}");
        }
    }

    /// len < MIN_PASSWORD_LEN → 钳到 10;len >= MIN_PASSWORD_LEN → 原样返回
    #[test]
    fn generate_strong_password_clamps_to_min_length() {
        let short = generate_strong_password(4);
        assert_eq!(short.len(), MIN_PASSWORD_LEN);

        let exact = generate_strong_password(20);
        assert_eq!(exact.len(), 20);
    }

    /// generate_strong_password 输出必须只落在四类字符合集里(含特殊符号集合精确锁定)
    #[test]
    fn generate_strong_password_emits_only_whitelisted_chars() {
        let pwd = generate_strong_password(64);
        for (idx, b) in pwd.bytes().enumerate() {
            let allowed = UPPERCASE.contains(&b)
                || LOWERCASE.contains(&b)
                || DIGITS.contains(&b)
                || SPECIALS.contains(&b);
            assert!(
                allowed,
                "byte {idx} = 0x{b:02x} ({:?}) not in whitelist (pwd={pwd:?})",
                b as char
            );
        }
    }

    /// SnowLumaSession 字节级 round-trip:camelCase 字段 + 二次序列化字节相等
    #[test]
    fn snowluma_session_round_trips_camel_case_fields() {
        let session = SnowLumaSession {
            password: "P@ssw0rd!".to_string(),
            created_at: "2024-01-01T00:00:00.000Z".to_string(),
            last_rendered_at: "2024-12-31T23:59:59.999Z".to_string(),
        };

        // 第一次序列化必须含 camelCase 字段名 + 明文 password
        let json1 = serde_json::to_string(&session).expect("serialize");
        assert!(
            json1.contains("\"createdAt\""),
            "missing createdAt in {json1}"
        );
        assert!(
            json1.contains("\"lastRenderedAt\""),
            "missing lastRenderedAt in {json1}"
        );
        assert!(
            json1.contains("\"password\""),
            "missing password in {json1}"
        );
        // 反向锁:snake_case 字段名不得出现
        assert!(
            !json1.contains("\"created_at\""),
            "snake_case leaked in {json1}"
        );
        assert!(
            !json1.contains("\"last_rendered_at\""),
            "snake_case leaked in {json1}"
        );

        // 反序列化 → 等价
        let parsed: SnowLumaSession = serde_json::from_str(&json1).expect("deserialize");
        assert_eq!(parsed, session);

        // 再序列化字节相等
        let json2 = serde_json::to_string(&parsed).expect("re-serialize");
        assert_eq!(json1, json2, "round-trip not byte-equal");
    }

    /// 首启写入 session.json;再次调用直接读,密码与 createdAt 全部稳定
    #[test]
    fn load_or_create_session_is_idempotent_after_first_call() {
        let temp = ncd_test_support::TempWorkspace::new().expect("tempdir");
        let root = temp.path();

        let first = load_or_create_session(root).expect("first");
        let second = load_or_create_session(root).expect("second");

        assert_eq!(first.password, second.password);
        assert_eq!(first.created_at, second.created_at);
        assert_eq!(first.last_rendered_at, second.last_rendered_at);

        // 文件确实落在 <root>/session.json
        assert!(session_path(root).exists());
    }

    /// 同一明文密码连续两次构造 payload:salt 与 hash 必须双双不同
    #[test]
    fn build_webui_json_payload_uses_different_salts() {
        let pwd = "p@SsW0rd-1234";
        let a = build_webui_json_payload(pwd, false).expect("payload a");
        let b = build_webui_json_payload(pwd, false).expect("payload b");

        let salt_a = a.get("passwordSalt").and_then(|v| v.as_str()).unwrap();
        let salt_b = b.get("passwordSalt").and_then(|v| v.as_str()).unwrap();
        assert_ne!(salt_a, salt_b, "salts collided: {salt_a} == {salt_b}");

        let hash_a = a.get("passwordHash").and_then(|v| v.as_str()).unwrap();
        let hash_b = b.get("passwordHash").and_then(|v| v.as_str()).unwrap();
        assert_ne!(hash_a, hash_b, "hashes collided: {hash_a} == {hash_b}");
    }

    /// 验证 now_iso8601() 输出(透过 load_or_create_session.created_at)严格匹配
    /// ^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}\.\d{3}Z$无 regex crate,手写 24 字符校验
    #[test]
    fn now_iso8601_matches_iso_8601_format() {
        let temp = ncd_test_support::TempWorkspace::new().expect("tempdir");
        let session = load_or_create_session(temp.path()).expect("session");
        assert_iso8601_millis(&session.created_at);
        assert_iso8601_millis(&session.last_rendered_at);
    }

    /// 手写 ISO 8601 毫秒精度 UTC 校验器:
    /// 0..=3 位年,-,2 位月,-,2 位日,T,2 位时,:,2 位分,:,2 位秒,.,3 位毫秒,Z
    fn assert_iso8601_millis(s: &str) {
        assert_eq!(s.len(), 24, "expected 24 chars, got {}: {s:?}", s.len());
        let bytes = s.as_bytes();
        // 数字/分隔符位置查表:
        // 0 1 2 3 - 5 6 - 8 9 T 11 12 : 14 15 : 17 18 . 20 21 22 Z
        let digit_positions = [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18, 20, 21, 22];
        for &i in &digit_positions {
            let b = bytes[i];
            assert!(
                b.is_ascii_digit(),
                "byte {i} = 0x{b:02x} not digit (s={s:?})"
            );
        }
        assert_eq!(bytes[4], b'-', "expected '-' at 4 (s={s:?})");
        assert_eq!(bytes[7], b'-', "expected '-' at 7 (s={s:?})");
        assert_eq!(bytes[10], b'T', "expected 'T' at 10 (s={s:?})");
        assert_eq!(bytes[13], b':', "expected ':' at 13 (s={s:?})");
        assert_eq!(bytes[16], b':', "expected ':' at 16 (s={s:?})");
        assert_eq!(bytes[19], b'.', "expected '.' at 19 (s={s:?})");
        assert_eq!(bytes[23], b'Z', "expected 'Z' at 23 (s={s:?})");
    }

    /// update_last_rendered 必须把时间戳推进到 ≥ 原值,并且大概率严格 >(毫秒精度时钟)
    #[test]
    fn update_last_rendered_advances_timestamp() {
        let temp = ncd_test_support::TempWorkspace::new().expect("tempdir");
        let root = temp.path();

        let original = load_or_create_session(root).expect("create session");
        // 至少跨越 1ms tick,留余量给 Windows 时钟分辨率(典型 ~15ms)
        std::thread::sleep(std::time::Duration::from_millis(50));

        update_last_rendered(root).expect("update last rendered");
        let reloaded = load_or_create_session(root).expect("reload");

        // ISO 8601 毫秒精度 UTC 字符串等宽,ascii 字典序 == 时间序
        assert!(
            reloaded.last_rendered_at.as_str() >= original.last_rendered_at.as_str(),
            "expected {:?} >= {:?}",
            reloaded.last_rendered_at,
            original.last_rendered_at
        );
        assert_ne!(
            reloaded.last_rendered_at, original.last_rendered_at,
            "timestamp did not advance after 50ms sleep"
        );
        // 密码 / createdAt 不应被覆盖
        assert_eq!(reloaded.password, original.password);
        assert_eq!(reloaded.created_at, original.created_at);
    }

    /// render_daemon_globals §3.5 优先级:override 非空白 → 用 override;否则 fallback 到
    /// session 密码无论走哪条分支,runtime.json / webui.json 都应原子落盘
    #[test]
    fn render_daemon_globals_uses_override_when_present() {
        let snow_dir = ncd_test_support::TempWorkspace::new().expect("snow tempdir");
        let runtime_dir = ncd_test_support::TempWorkspace::new().expect("runtime tempdir");
        let snow = snow_dir.path();
        let runtime = runtime_dir.path();

        // 1) override = Some("OVERRIDE!@123") → 返回 override 原值
        let override_pwd = "OVERRIDE!@123";
        let returned = render_daemon_globals(snow, runtime, Some(override_pwd), 5099)
            .expect("render override");
        assert_eq!(returned, override_pwd);

        // runtime.json 内容锁:仅 webuiPort 一个字段且为 5099
        let runtime_json = std::fs::read_to_string(runtime.join("config").join("runtime.json"))
            .expect("read runtime.json");
        let parsed: serde_json::Value =
            serde_json::from_str(&runtime_json).expect("parse runtime.json");
        assert_eq!(parsed["webuiPort"], serde_json::json!(5099));

        // webui.json 必须存在
        let webui_path = runtime.join("config").join("webui.json");
        assert!(webui_path.exists(), "webui.json not written");

        // 2) override = None → 走 session 路径,返回 session 密码
        let session = load_or_create_session(snow).expect("read session for assertion");
        let returned_none = render_daemon_globals(snow, runtime, None, 5099).expect("render none");
        assert_eq!(returned_none, session.password);

        // 3) override = Some("") / Some(" ") → trim 后空,等同 fallback 到 session
        for empty in ["", " ", "\t\n"] {
            let r = render_daemon_globals(snow, runtime, Some(empty), 5099)
                .unwrap_or_else(|e| panic!("render empty {empty:?}: {e:?}"));
            assert_eq!(
                r, session.password,
                "empty/whitespace override should fallback to session (input={empty:?})"
            );
        }
    }
