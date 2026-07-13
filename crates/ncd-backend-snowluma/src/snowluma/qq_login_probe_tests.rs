    use super::*;

    /// JWT payload 解码:标准 base64url 用例这条字面量是从 NapCat 实际响应里
    /// 截取的 payload 二段,errCode=0,uin=572381217
    #[test]
    fn decode_jwt_payload_extracts_uin() {
        // payload = {"errCode":0,"uin":"572381217"}
        // base64url(unpadded) = eyJlcnJDb2RlIjowLCJ1aW4iOiI1NzIzODEyMTcifQ
        let token = "eyJ.eyJlcnJDb2RlIjowLCJ1aW4iOiI1NzIzODEyMTcifQ.sig";
        let payload = decode_jwt_payload(token).expect("decode payload");
        assert_eq!(payload.err_code, 0);
        assert_eq!(payload.uin.as_deref(), Some("572381217"));
    }

    /// extract_jwt 应当从混杂的 HTTP 响应文本里精确切出第一段 JWT
    #[test]
    fn extract_jwt_finds_first_three_segment_token() {
        let body = "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\n\r\n\
                    {\"data\":\"eyJh.eyJiIjoxfQ.s_ig\"}";
        let jwt = extract_jwt(body).expect("jwt match");
        assert_eq!(jwt, "eyJh.eyJiIjoxfQ.s_ig");
    }

    /// 没有合法 JWT 时返回 None,不抛错
    #[test]
    fn extract_jwt_returns_none_when_absent() {
        assert!(extract_jwt("plain text without token").is_none());
        // 只有两段不算 JWT
        assert!(extract_jwt("eyJa.eyJb").is_none());
    }

    /// JSONP 解析:从 Ptlogin2 真实返回形态里抠出账号数组
    #[test]
    fn parse_ptlogin_accounts_extracts_array() {
        let body = "var var_sso_uin_list=[{\"uin\":\"572381217\",\"nickname\":\"a\"}];\
                    ptui_getuins_CB(var_sso_uin_list)";
        let accts = parse_ptlogin_accounts(body);
        assert_eq!(accts.len(), 1);
        assert_eq!(accts[0].uin_string(), "572381217");
    }

    /// uin 为数字字面量也能解(不同 QQ 版本返回类型不一致)
    #[test]
    fn parse_ptlogin_accounts_accepts_numeric_uin() {
        let body = "x=[{\"uin\":572381217},{\"account\":10001}]";
        let accts = parse_ptlogin_accounts(body);
        assert_eq!(accts.len(), 2);
        assert_eq!(accts[0].uin_string(), "572381217");
        // 第二条 uin 缺失回退 account
        assert_eq!(accts[1].uin_string(), "10001");
    }

    /// 无方括号 / 非法 JSON → 空列表,不 panic
    #[test]
    fn parse_ptlogin_accounts_returns_empty_on_garbage() {
        assert!(parse_ptlogin_accounts("no brackets here").is_empty());
        assert!(parse_ptlogin_accounts("[not valid json}").is_empty());
        assert!(parse_ptlogin_accounts("").is_empty());
    }

    fn acct(uin: &str) -> PtloginAccount {
        PtloginAccount {
            uin: Some(UinField::Str(uin.to_string())),
            account: None,
        }
    }

    /// 双发包对齐:情况一 2+1,取较短组的当前账号
    #[test]
    fn select_current_uin_picks_shorter_when_two_plus_one() {
        let two = vec![acct("100"), acct("200")];
        let one = vec![acct("300")];
        assert_eq!(select_current_uin(&two, &one).as_deref(), Some("300"));
        // 顺序无关
        assert_eq!(select_current_uin(&one, &two).as_deref(), Some("300"));
    }

    /// 双发包对齐:情况二 1+1,两次都是当前账号
    #[test]
    fn select_current_uin_picks_when_one_plus_one() {
        let a = vec![acct("572381217")];
        let b = vec![acct("572381217")];
        assert_eq!(select_current_uin(&a, &b).as_deref(), Some("572381217"));
    }

    /// 双发包对齐:情况三 2+0(当前账号非前两个登录账号),放弃转兜底
    #[test]
    fn select_current_uin_gives_up_when_two_plus_zero() {
        let two = vec![acct("100"), acct("200")];
        let zero: Vec<PtloginAccount> = vec![];
        assert!(select_current_uin(&two, &zero).is_none());
        assert!(select_current_uin(&zero, &two).is_none());
    }

    /// 双发包对齐:都返回 2 条(异常轮换),无法锁定,放弃
    #[test]
    fn select_current_uin_gives_up_when_both_two() {
        let a = vec![acct("100"), acct("200")];
        let b = vec![acct("100"), acct("200")];
        assert!(select_current_uin(&a, &b).is_none());
    }

    /// 较短组虽为 1 条但 uin 为空 → 不返回空串,放弃
    #[test]
    fn select_current_uin_rejects_empty_uin() {
        let empty = vec![acct("")];
        let two = vec![acct("100"), acct("200")];
        assert!(select_current_uin(&empty, &two).is_none());
    }
