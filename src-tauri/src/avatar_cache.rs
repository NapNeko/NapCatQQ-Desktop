//! QQ / 群头像的内存 + 磁盘缓存，原生托盘面板用；磁盘文件和账号托盘图标共用 state/chat/avatars。

use std::collections::HashMap;
use std::future::Future;
use std::io::Cursor;
use std::path::Path;
use std::sync::{Arc, LazyLock, Mutex, Weak};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use tauri::{AppHandle, Manager};

const SIZE: u32 = 128;
const MAX_BYTES: usize = 512 * 1024;
const MAX_ENTRIES: usize = 128;
pub const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
pub const FAILURE_TTL: Duration = Duration::from_secs(60);
const TIMEOUT: Duration = Duration::from_secs(8);

#[derive(Clone, PartialEq, Eq, Hash, Debug)]
enum Kind {
    User(String),
    Group(String),
}

/// 头像的身份。只能经 [`AvatarKey::user`] / [`AvatarKey::group`] 构造：号码会拼进 URL 和缓存文件名，
/// 校验是路径安全的边界。
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct AvatarKey(Kind);

fn valid_id(id: &str) -> bool {
    // 对齐前端 avatarUrl 的 id >= 10000，且不以 0 开头
    id.len() >= 5
        && id.len() <= 20
        && id.bytes().all(|b| b.is_ascii_digit())
        && !id.starts_with('0')
}

impl AvatarKey {
    pub fn user(id: &str) -> Option<Self> {
        let id = id.trim();
        valid_id(id).then(|| Self(Kind::User(id.to_owned())))
    }

    pub fn group(id: &str) -> Option<Self> {
        let id = id.trim();
        valid_id(id).then(|| Self(Kind::Group(id.to_owned())))
    }

    fn url(&self, now: SystemTime) -> String {
        let base = match &self.0 {
            Kind::User(qq) => format!("https://q1.qlogo.cn/g?b=qq&nk={qq}&s=640"),
            Kind::Group(gid) => format!("https://p.qlogo.cn/gh/{gid}/{gid}/640"),
        };
        let separator = if base.contains('?') { '&' } else { '?' };
        let version =
            now.duration_since(UNIX_EPOCH).unwrap_or_default().as_secs() / CACHE_TTL.as_secs();
        format!("{base}{separator}_ncdAvatar={version}")
    }

    fn file_name(&self) -> String {
        match &self.0 {
            // 和 chat_tray 的托盘图标同名，互相命中缓存
            Kind::User(qq) => format!("{qq}-{SIZE}.png"),
            Kind::Group(gid) => format!("group-{gid}-{SIZE}.png"),
        }
    }
}

/// 128×128 非预乘 RGBA。
pub struct AvatarPixels {
    pub size: u32,
    pub rgba: Vec<u8>,
}

#[derive(Clone)]
struct CachedAvatar {
    pixels: Arc<AvatarPixels>,
    expires_at: Instant,
}

static CACHE: LazyLock<Mutex<HashMap<AvatarKey, CachedAvatar>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static FAILED: LazyLock<Mutex<HashMap<AvatarKey, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DOWNLOADS: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(4));
static LOADS: LazyLock<Mutex<HashMap<AvatarKey, Weak<tokio::sync::Mutex<()>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

fn load_lock(key: &AvatarKey) -> Arc<tokio::sync::Mutex<()>> {
    let mut loads = lock(&LOADS);
    loads.retain(|_, load| load.strong_count() > 0);
    if let Some(load) = loads.get(key).and_then(Weak::upgrade) {
        return load;
    }
    let load = Arc::new(tokio::sync::Mutex::new(()));
    loads.insert(key.clone(), Arc::downgrade(&load));
    load
}

fn disk_remaining(modified: Option<SystemTime>, now: SystemTime) -> Duration {
    modified
        .and_then(|at| now.duration_since(at).ok())
        .and_then(|age| CACHE_TTL.checked_sub(age))
        .unwrap_or_default()
}

fn remember(key: &AvatarKey, pixels: Arc<AvatarPixels>, remaining: Duration) {
    let mut cache = lock(&CACHE);
    if cache.len() >= MAX_ENTRIES && !cache.contains_key(key) {
        cache.clear();
    }
    cache.insert(
        key.clone(),
        CachedAvatar {
            pixels,
            expires_at: Instant::now() + remaining,
        },
    );
}

pub fn refresh_after(key: &AvatarKey) -> Duration {
    let now = Instant::now();
    let retry = lock(&FAILED)
        .get(key)
        .copied()
        .and_then(|at| FAILURE_TTL.checked_sub(now.saturating_duration_since(at)));
    retry
        .unwrap_or_else(|| {
            lock(&CACHE)
                .get(key)
                .map(|entry| entry.expires_at.saturating_duration_since(now))
                .unwrap_or(FAILURE_TTL)
        })
        .max(Duration::from_secs(1))
}

/// 过期后重新下载；暂时断网时保留上次成功的头像。
pub async fn load(app: &AppHandle, key: &AvatarKey) -> Option<Arc<AvatarPixels>> {
    let root = app
        .try_state::<crate::AppState>()?
        .data_root
        .join("state/chat/avatars");
    let url = key.url(SystemTime::now());
    load_cached(&root, key, download(&url)).await
}

async fn load_cached(
    root: &Path,
    key: &AvatarKey,
    fetch: impl Future<Output = Option<Vec<u8>>>,
) -> Option<Arc<AvatarPixels>> {
    let loading = load_lock(key);
    let _loading = loading.lock().await;
    let cached = lock(&CACHE).get(key).cloned();
    if let Some(hit) = cached
        .as_ref()
        .filter(|hit| hit.expires_at > Instant::now())
    {
        return Some(Arc::clone(&hit.pixels));
    }
    let path = root.join(key.file_name());
    let mut previous = cached.map(|hit| hit.pixels);
    if previous.is_none() {
        let modified = tokio::fs::metadata(&path)
            .await
            .ok()
            .and_then(|metadata| metadata.modified().ok());
        let bytes = tokio::fs::read(&path)
            .await
            .ok()
            .filter(|bytes| !bytes.is_empty() && bytes.len() <= MAX_BYTES);
        if let Some((pixels, _)) = decode_async(bytes).await {
            let pixels = Arc::new(pixels);
            let remaining = disk_remaining(modified, SystemTime::now());
            remember(key, Arc::clone(&pixels), remaining);
            if !remaining.is_zero() {
                return Some(pixels);
            }
            previous = Some(pixels);
        }
    }
    if lock(&FAILED)
        .get(key)
        .is_some_and(|at| at.elapsed() < FAILURE_TTL)
    {
        return previous;
    }
    let bytes = fetch.await;
    let Some((pixels, png)) = decode_async(bytes).await else {
        let mut failed = lock(&FAILED);
        if failed.len() >= MAX_ENTRIES {
            failed.clear();
        }
        failed.insert(key.clone(), Instant::now());
        return previous;
    };
    if tokio::fs::create_dir_all(&root).await.is_ok() {
        // 只有下载成功才替换文件；读旧图不能延长它的磁盘有效期。
        let tmp = root.join(format!("{}.tmp", key.file_name()));
        if tokio::fs::write(&tmp, png).await.is_ok()
            && tokio::fs::rename(&tmp, &path).await.is_err()
        {
            let _ = tokio::fs::remove_file(&tmp).await;
        }
    }
    let pixels = Arc::new(pixels);
    lock(&FAILED).remove(key);
    remember(key, Arc::clone(&pixels), CACHE_TTL);
    Some(pixels)
}

async fn decode_async(bytes: Option<Vec<u8>>) -> Option<(AvatarPixels, Vec<u8>)> {
    let bytes = bytes?;
    tokio::task::spawn_blocking(move || decode(bytes))
        .await
        .ok()
        .flatten()
}

async fn download(url: &str) -> Option<Vec<u8>> {
    // 许可只包下载这一步，读盘和解码不占
    let _permit = DOWNLOADS.acquire().await.ok()?;
    let fetch = async {
        let mut response = ncd_network::shared_client()
            .get(url)
            .header("Cache-Control", "no-cache")
            .timeout(TIMEOUT)
            .send()
            .await
            .ok()?
            .error_for_status()
            .ok()?;
        let mut bytes = Vec::new();
        while let Some(chunk) = response.chunk().await.ok()? {
            if bytes.len() + chunk.len() > MAX_BYTES {
                return None;
            }
            bytes.extend_from_slice(&chunk);
        }
        Some(bytes)
    };
    tokio::time::timeout(TIMEOUT, fetch).await.ok().flatten()
}

/// 解码并缩到 128×128；同时给出要写回磁盘的 PNG。
fn decode(bytes: Vec<u8>) -> Option<(AvatarPixels, Vec<u8>)> {
    let mut reader = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .ok()?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(2048);
    limits.max_image_height = Some(2048);
    limits.max_alloc = Some(16 * 1024 * 1024);
    reader.limits(limits);
    let rgba = reader
        .decode()
        .ok()?
        .resize_exact(SIZE, SIZE, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let mut png = Cursor::new(Vec::new());
    image::DynamicImage::ImageRgba8(rgba.clone())
        .write_to(&mut png, image::ImageFormat::Png)
        .ok()?;
    Some((
        AvatarPixels {
            size: SIZE,
            rgba: rgba.into_raw(),
        },
        png.into_inner(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_only_accept_plain_qq_numbers() {
        assert!(AvatarKey::user("2707600964").is_some());
        assert!(AvatarKey::group(" 20001 ").is_some());
        assert!(AvatarKey::user("0123456").is_none());
        assert!(AvatarKey::user("1234").is_none());
        assert!(AvatarKey::user("12345/../x").is_none());
        assert!(AvatarKey::user("").is_none());
        let key = AvatarKey::group("20001").unwrap();
        assert_eq!(key.file_name(), "group-20001-128.png");
        assert_eq!(
            key.url(UNIX_EPOCH),
            "https://p.qlogo.cn/gh/20001/20001/640?_ncdAvatar=0"
        );
    }

    fn png(color: u8) -> Vec<u8> {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            2,
            2,
            image::Rgba([color, 20, 30, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        png.into_inner()
    }

    #[test]
    fn urls_use_the_same_five_minute_version_for_users_and_groups() {
        for key in [
            AvatarKey::user("100001").unwrap(),
            AvatarKey::group("100001").unwrap(),
        ] {
            assert_eq!(
                key.url(UNIX_EPOCH),
                key.url(UNIX_EPOCH + CACHE_TTL - Duration::from_secs(1))
            );
            assert_ne!(key.url(UNIX_EPOCH), key.url(UNIX_EPOCH + CACHE_TTL));
            assert!(key.url(UNIX_EPOCH + CACHE_TTL).ends_with("_ncdAvatar=1"));
        }
    }

    #[test]
    fn disk_expiry_keeps_original_age_and_rejects_future_timestamps() {
        let now = UNIX_EPOCH + Duration::from_secs(1000);
        assert_eq!(
            disk_remaining(Some(now - Duration::from_secs(120)), now),
            Duration::from_secs(180)
        );
        assert_eq!(disk_remaining(Some(now - CACHE_TTL), now), Duration::ZERO);
        assert_eq!(
            disk_remaining(Some(now + Duration::from_secs(1)), now),
            Duration::ZERO
        );
        assert_eq!(disk_remaining(None, now), Duration::ZERO);
    }

    #[tokio::test]
    async fn disk_and_memory_hits_do_not_renew_the_original_expiry() {
        let root = tempfile::tempdir().unwrap();
        let key = AvatarKey::user("930001").unwrap();
        let path = root.path().join(key.file_name());
        std::fs::write(&path, png(10)).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(SystemTime::now() - Duration::from_secs(120)),
            )
            .unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let first = load_cached(root.path(), &key, async {
            panic!("fresh disk must not download")
        })
        .await
        .unwrap();
        let expires_at = lock(&CACHE).get(&key).unwrap().expires_at;
        assert!(expires_at <= Instant::now() + Duration::from_secs(181));
        let second = load_cached(root.path(), &key, async {
            panic!("fresh memory must not download")
        })
        .await
        .unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(lock(&CACHE).get(&key).unwrap().expires_at, expires_at);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
    }

    #[tokio::test]
    async fn failed_refresh_keeps_old_disk_image_then_retries_after_cooldown() {
        let root = tempfile::tempdir().unwrap();
        let key = AvatarKey::user("930002").unwrap();
        let path = root.path().join(key.file_name());
        std::fs::write(&path, png(10)).unwrap();
        std::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(SystemTime::now() - CACHE_TTL - Duration::from_secs(1)),
            )
            .unwrap();
        let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
        let previous = load_cached(root.path(), &key, async { None })
            .await
            .unwrap();
        assert_eq!(previous.rgba[0], 10);
        assert_eq!(
            std::fs::metadata(&path).unwrap().modified().unwrap(),
            modified
        );
        assert!(refresh_after(&key) <= FAILURE_TTL);
        let cooled = load_cached(root.path(), &key, async {
            panic!("failure cooldown must suppress download")
        })
        .await
        .unwrap();
        assert!(Arc::ptr_eq(&previous, &cooled));
        lock(&FAILED).insert(
            key.clone(),
            Instant::now() - FAILURE_TTL - Duration::from_secs(1),
        );
        let refreshed = load_cached(root.path(), &key, async { Some(png(80)) })
            .await
            .unwrap();
        assert_eq!(refreshed.rgba[0], 80);
        assert!(!lock(&FAILED).contains_key(&key));
    }

    #[tokio::test]
    async fn simultaneous_requests_only_download_once() {
        let root = tempfile::tempdir().unwrap();
        let key = AvatarKey::user("930003").unwrap();
        let calls = std::sync::atomic::AtomicUsize::new(0);
        let fetch = || async {
            calls.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            tokio::task::yield_now().await;
            Some(png(60))
        };
        let (first, second) = tokio::join!(
            load_cached(root.path(), &key, fetch()),
            load_cached(root.path(), &key, fetch()),
        );
        assert!(Arc::ptr_eq(&first.unwrap(), &second.unwrap()));
        assert_eq!(calls.load(std::sync::atomic::Ordering::Relaxed), 1);
    }

    #[test]
    fn decode_scales_to_fixed_size() {
        let mut png = Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image::RgbaImage::from_pixel(
            40,
            20,
            image::Rgba([10, 20, 30, 255]),
        ))
        .write_to(&mut png, image::ImageFormat::Png)
        .unwrap();
        let (pixels, _) = decode(png.into_inner()).unwrap();
        assert_eq!(pixels.size, SIZE);
        assert_eq!(pixels.rgba.len(), (SIZE * SIZE * 4) as usize);
    }
}
