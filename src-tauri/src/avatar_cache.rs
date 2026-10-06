//! QQ / 群头像的内存 + 磁盘缓存，原生托盘面板用；磁盘文件和账号托盘图标共用 state/chat/avatars。

use std::collections::HashMap;
use std::io::Cursor;
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use tauri::{AppHandle, Manager};

const SIZE: u32 = 128;
const MAX_BYTES: usize = 512 * 1024;
const MAX_ENTRIES: usize = 128;
// 和前端 ChatAvatar 的失败缓存一致：拉不到的号 5 分钟内不再打网络
const FAILURE_TTL: Duration = Duration::from_secs(5 * 60);
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

    fn url(&self) -> String {
        match &self.0 {
            Kind::User(qq) => format!("https://q1.qlogo.cn/g?b=qq&nk={qq}&s=640"),
            Kind::Group(gid) => format!("https://p.qlogo.cn/gh/{gid}/{gid}/640"),
        }
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

static CACHE: LazyLock<Mutex<HashMap<AvatarKey, Arc<AvatarPixels>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static FAILED: LazyLock<Mutex<HashMap<AvatarKey, Instant>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));
static DOWNLOADS: LazyLock<tokio::sync::Semaphore> =
    LazyLock::new(|| tokio::sync::Semaphore::new(4));

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|p| p.into_inner())
}

/// 取头像：内存缓存 → 失败冷却期 → 磁盘缓存 → 下载。拿不到返回 None，调用方画占位。
pub async fn load(app: &AppHandle, key: &AvatarKey) -> Option<Arc<AvatarPixels>> {
    if let Some(hit) = lock(&CACHE).get(key).cloned() {
        return Some(hit);
    }
    if lock(&FAILED)
        .get(key)
        .is_some_and(|at| at.elapsed() < FAILURE_TTL)
    {
        return None;
    }
    let root = app
        .try_state::<crate::AppState>()?
        .data_root
        .join("state/chat/avatars");
    let path = root.join(key.file_name());
    let cached = tokio::fs::read(&path)
        .await
        .ok()
        .filter(|b| !b.is_empty() && b.len() <= MAX_BYTES);
    let from_disk = cached.is_some();
    let bytes = match cached {
        Some(bytes) => Some(bytes),
        None => download(&key.url()).await,
    };
    let decoded = match bytes {
        Some(bytes) => tokio::task::spawn_blocking(move || decode(bytes))
            .await
            .ok()
            .flatten(),
        None => None,
    };
    let Some((pixels, png)) = decoded else {
        let mut failed = lock(&FAILED);
        if failed.len() >= MAX_ENTRIES {
            failed.clear();
        }
        failed.insert(key.clone(), Instant::now());
        return None;
    };
    if !from_disk && tokio::fs::create_dir_all(&root).await.is_ok() {
        // 先写临时文件再改名：托盘图标那边同时在读同一个文件时不会读到半截
        let tmp = root.join(format!("{}.tmp", key.file_name()));
        if tokio::fs::write(&tmp, png).await.is_ok()
            && tokio::fs::rename(&tmp, &path).await.is_err()
        {
            let _ = tokio::fs::remove_file(&tmp).await;
        }
    }
    let pixels = Arc::new(pixels);
    let mut cache = lock(&CACHE);
    if cache.len() >= MAX_ENTRIES {
        cache.clear();
    }
    cache.insert(key.clone(), Arc::clone(&pixels));
    Some(pixels)
}

async fn download(url: &str) -> Option<Vec<u8>> {
    // 许可只包下载这一步，读盘和解码不占
    let _permit = DOWNLOADS.acquire().await.ok()?;
    let fetch = async {
        let mut response = ncd_network::shared_client()
            .get(url)
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
        assert_eq!(key.url(), "https://p.qlogo.cn/gh/20001/20001/640");
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
