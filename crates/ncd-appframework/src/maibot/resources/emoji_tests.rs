use super::*;
use wiremock::matchers::{header, method, path, query_param};
use wiremock::{Match, Mock, MockServer, ResponseTemplate};

const PNG: [u8; 10] = [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0];
const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00";

/// multipart 里夹着图片字节，不是合法 UTF-8，wiremock 的 body_string_contains 一律不中；按字节找
struct BodyHas(&'static [u8]);

impl Match for BodyHas {
    fn matches(&self, req: &wiremock::Request) -> bool {
        req.body.windows(self.0.len()).any(|w| w == self.0)
    }
}

#[test]
fn tags_split_like_upstream_and_dedupe() {
    assert_eq!(
        normalize_tags(&["开心，得意、 嘿嘿;开心".into(), " 得意 ".into()]),
        ["开心", "得意", "嘿嘿"]
    );
    assert!(normalize_tags(&["  ,，".into()]).is_empty());
}

#[test]
fn upstream_rows_map_to_items() {
    let raw = r#"{"success":true,"total":3,"page":1,"page_size":20,"data":[
        {"id":7,"full_path":"data/emoji/a.gif","format":"gif","emoji_hash":"h7","description":"开心,得意",
         "query_count":3,"usage_count":3,"is_registered":true,"is_banned":false,"status":"adopted",
         "emotion":"开心,得意","record_time":1790000000.0,"register_time":1790000100.0,"last_used_time":null},
        {"id":8,"format":"png","emoji_hash":"h8","description":"","is_registered":false,"is_banned":false,"record_time":0.0},
        {"id":9,"format":"png","emoji_hash":"h9","description":"无语","is_registered":false,"is_banned":true}
    ]}"#;
    let up: UpstreamPage<UpstreamEmoji> = serde_json::from_str(raw).unwrap();
    let items: Vec<_> = up
        .data
        .unwrap()
        .into_iter()
        .filter_map(UpstreamEmoji::into_item)
        .collect();
    assert_eq!(items[0].tags, ["开心", "得意"]);
    assert_eq!(
        (items[0].status, items[0].usage_count),
        (MaiBotEmojiStatus::Adopted, 3)
    );
    assert_eq!(items[0].adopted_at, Some(1790000100.0));
    // 老版本没有 status 字段：按上游同样的规则推
    assert_eq!(items[1].status, MaiBotEmojiStatus::Unknown);
    assert_eq!(items[1].found_at, None, "0.0 是上游没记时间");
    assert_eq!(items[2].status, MaiBotEmojiStatus::Discarded);
}

#[test]
fn sniffs_only_the_four_formats_upstream_takes() {
    assert_eq!(sniff_image(&PNG), Some("image/png"));
    assert_eq!(sniff_image(&[0xFF, 0xD8, 0xFF, 0xE0]), Some("image/jpeg"));
    assert_eq!(sniff_image(GIF), Some("image/gif"));
    assert_eq!(sniff_image(b"RIFF\0\0\0\0WEBPVP8 "), Some("image/webp"));
    assert_eq!(sniff_image(b"BM\0\0\0\0"), None);
    assert_eq!(sniff_image(b"RIFF\0\0\0\0WAVE"), None);
}

#[tokio::test]
async fn local_images_are_checked_before_upload() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("a.png");
    std::fs::write(&png, PNG).unwrap();
    let fake = dir.path().join("b.png");
    std::fs::write(&fake, b"not an image").unwrap();
    let empty = dir.path().join("c.gif");
    std::fs::write(&empty, b"").unwrap();
    let s = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let paths = vec![
        s(&png),
        s(&fake),
        s(&empty),
        "relative/x.png".into(),
        s(dir.path()),
        s(&png),
    ];

    let got = inspect_local_images(paths).await;
    assert_eq!(got.len(), 5, "同一张拖进来两次只算一张");
    assert_eq!(
        (got[0].name.as_str(), got[0].size, got[0].problem.as_deref()),
        ("a.png", 10, None)
    );
    assert!(
        got[0]
            .preview
            .as_deref()
            .unwrap()
            .starts_with("data:image/png;base64,")
    );
    let problems: Vec<_> = got[1..]
        .iter()
        .map(|g| g.problem.as_deref().unwrap())
        .collect();
    assert_eq!(
        problems,
        [
            "不是 PNG / JPG / GIF / WebP 图片",
            "是个空文件",
            "路径不对",
            "不是文件"
        ]
    );
    assert!(got[1..].iter().all(|g| g.preview.is_none()));
}

fn client(server: &MockServer) -> MaiBotWebUi {
    MaiBotWebUi::connect(server.address().port(), "tok").unwrap()
}

#[tokio::test]
async fn thumbnail_waits_out_202_then_returns_a_data_url() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/webui/emoji/5/thumbnail"))
        .and(header("cookie", "maibot_session=tok"))
        .respond_with(ResponseTemplate::new(202).set_body_json(json!({ "status": "generating" })))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/api/webui/emoji/5/thumbnail"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", "image/webp")
                .set_body_bytes(b"RIFF\0\0\0\0WEBPVP8 ".to_vec()),
        )
        .mount(&server)
        .await;
    let img = image(&client(&server), 5, false).await.unwrap();
    assert!(
        img.data_url
            .unwrap()
            .starts_with("data:image/webp;base64,UklGR")
    );
}

#[tokio::test]
async fn original_of_a_cleaned_file_is_just_missing() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/api/webui/emoji/6/thumbnail"))
        .and(query_param("original", "true"))
        .respond_with(
            ResponseTemplate::new(404).set_body_json(json!({ "detail": "表情包文件不存在" })),
        )
        .mount(&server)
        .await;
    assert_eq!(
        image(&client(&server), 6, true).await.unwrap().data_url,
        None
    );
}

#[tokio::test]
async fn upload_sends_the_sniffed_mime_and_tells_new_from_existing() {
    let dir = tempfile::tempdir().unwrap();
    // 扩展名故意写错：MIME 按文件头来
    let a = dir.path().join("a.jpg");
    std::fs::write(&a, PNG).unwrap();
    let b = dir.path().join("b.gif");
    std::fs::write(&b, GIF).unwrap();
    let bad = dir.path().join("c.webp");
    std::fs::write(&bad, b"nope").unwrap();

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/webui/emoji/upload"))
        .and(BodyHas(b"filename=\"a.jpg\""))
        .and(BodyHas(b"image/png"))
        .and(BodyHas("开心,得意".as_bytes()))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": true, "message": "表情包上传成功并已注册" })),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/webui/emoji/upload"))
        .and(BodyHas(b"filename=\"b.gif\""))
        .and(BodyHas(b"image/gif"))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(
                json!({ "success": true, "message": "表情包已存在，已标记为据为己用" }),
            ),
        )
        .expect(1)
        .mount(&server)
        .await;

    let s = |p: &std::path::Path| p.to_string_lossy().into_owned();
    let req = MaiBotEmojiUpload {
        paths: vec![s(&a), s(&b), s(&bad)],
        tags: vec!["开心 得意".into()],
    };
    let done = upload(&client(&server), &req).await.unwrap();
    assert_eq!((done.uploaded, done.existed), (1, 1));
    assert_eq!(
        done.failed,
        [MaiBotUploadFailure {
            name: "c.webp".into(),
            reason: "不是 PNG / JPG / GIF / WebP 图片".into()
        }]
    );
}

#[tokio::test]
async fn batch_adopt_reports_partial_failures() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/api/webui/emoji/1/register"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!({ "success": true, "message": "表情包注册成功" })),
        )
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/api/webui/emoji/2/register"))
        .respond_with(
            ResponseTemplate::new(400)
                .set_body_json(json!({ "detail": "表情包未通过内容审核，无法注册" })),
        )
        .mount(&server)
        .await;
    let c = client(&server);
    let done = act(&c, &MaiBotEmojiAction::Adopt { ids: vec![1, 2] })
        .await
        .unwrap();
    assert_eq!(done.affected, 1);
    assert_eq!(
        done.message,
        "收下了 1 张，1 张没成：表情包未通过内容审核，无法注册"
    );
    // 一张都没成：直接报错，页面按失败提示
    let err = act(&c, &MaiBotEmojiAction::Adopt { ids: vec![2] })
        .await
        .unwrap_err();
    assert!(err.to_string().contains("未通过内容审核"));
}
