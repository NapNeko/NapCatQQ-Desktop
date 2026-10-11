// 导出标准 CF_DIB 图片，发布成功后把内存所有权交给 Windows。
#![expect(unsafe_code, reason = "Win32 剪贴板和全局内存的 FFI 边界")]
#![warn(clippy::undocumented_unsafe_blocks)]

use std::marker::PhantomData;
use std::mem::size_of;
use std::rc::Rc;

use windows::Win32::Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND};
use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFOHEADER};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{
    GMEM_MOVEABLE, GMEM_ZEROINIT, GlobalAlloc, GlobalLock, GlobalUnlock,
};
use windows::Win32::UI::WindowsAndMessaging::IsWindow;

const CF_DIB: u32 = 8;
const MAX_PIXEL_BYTES: usize = 256 * 1024 * 1024;

pub fn copy_rgba(hwnd: HWND, width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    // SAFETY: hwnd 只用于系统查询；调用方让截图窗口存活至复制操作结束。
    if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err("截图窗口已经关闭".into());
    }
    let layout = DibLayout::new(width, height, rgba.len())?;
    let memory = GlobalMemory::new(layout.total_bytes)?;
    {
        let lock = memory.lock()?;
        let header = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: height as i32,
            biPlanes: 1,
            biBitCount: 24,
            biCompression: BI_RGB.0,
            biSizeImage: layout.pixel_bytes as u32,
            ..Default::default()
        };
        // SAFETY: 已锁定的分配包含完整头部和像素区，写入范围由 layout 校验。
        unsafe {
            std::ptr::write_unaligned(lock.ptr.cast::<BITMAPINFOHEADER>(), header);
            let pixels = std::slice::from_raw_parts_mut(
                lock.ptr.add(size_of::<BITMAPINFOHEADER>()),
                layout.pixel_bytes,
            );
            encode_bgr_rows(width as usize, height as usize, layout.stride, rgba, pixels);
        }
    }
    // 位图分配、编码和解锁均先完成，失败时保留原剪贴板内容。
    let _clipboard = ClipboardGuard::open(hwnd)?;
    // SAFETY: 当前线程独占打开剪贴板，内存已就绪且不再被写入。
    unsafe { EmptyClipboard() }.map_err(|error| format!("清空剪贴板失败：{error}"))?;
    // SAFETY: GMEM_MOVEABLE 内存已解锁；仅成功后转交系统，失败仍由守卫释放。
    unsafe { SetClipboardData(CF_DIB, Some(HANDLE(memory.handle.0))) }
        .map_err(|error| format!("复制截图失败：{error}"))?;
    memory.release();
    Ok(())
}

struct DibLayout {
    stride: usize,
    pixel_bytes: usize,
    total_bytes: usize,
}

impl DibLayout {
    fn new(width: u32, height: u32, rgba_len: usize) -> Result<Self, String> {
        if width == 0 || height == 0 || width > i32::MAX as u32 || height > i32::MAX as u32 {
            return Err("截图尺寸无效".into());
        }
        let input_bytes = (width as usize)
            .checked_mul(height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .filter(|bytes| *bytes <= MAX_PIXEL_BYTES)
            .ok_or("截图像素超过内存限制（256 MiB）")?;
        if rgba_len != input_bytes {
            return Err("截图像素长度与尺寸不符".into());
        }
        let stride = (width as usize)
            .checked_mul(3)
            .and_then(|bytes| bytes.checked_add(3))
            .map(|bytes| bytes & !3)
            .ok_or("剪贴板图片行宽无效")?;
        let pixel_bytes = stride
            .checked_mul(height as usize)
            .filter(|bytes| *bytes <= MAX_PIXEL_BYTES)
            .ok_or("剪贴板图片超过内存限制")?;
        let total_bytes = size_of::<BITMAPINFOHEADER>()
            .checked_add(pixel_bytes)
            .ok_or("剪贴板图片长度无效")?;
        Ok(Self {
            stride,
            pixel_bytes,
            total_bytes,
        })
    }
}

fn encode_bgr_rows(width: usize, height: usize, stride: usize, rgba: &[u8], output: &mut [u8]) {
    // 正向高度的 24 位底向上 DIB 兼容旧应用；各行填充由 GMEM_ZEROINIT 置零。
    for (row, destination) in output.chunks_exact_mut(stride).enumerate() {
        let source_row = height - row - 1;
        let source = &rgba[source_row * width * 4..(source_row + 1) * width * 4];
        for (pixel, bgr) in source.chunks_exact(4).zip(destination.chunks_exact_mut(3)) {
            bgr.copy_from_slice(&[pixel[2], pixel[1], pixel[0]]);
        }
    }
}

struct ClipboardGuard(PhantomData<Rc<()>>);

impl ClipboardGuard {
    fn open(hwnd: HWND) -> Result<Self, String> {
        for attempt in 0..5 {
            // SAFETY: 窗口在调用期间存活；仅成功打开时创建关闭守卫。
            match unsafe { OpenClipboard(Some(hwnd)) } {
                Ok(()) => return Ok(Self(PhantomData)),
                Err(error) if attempt == 4 => {
                    return Err(format!("剪贴板正在被占用，请重试：{error}"));
                }
                Err(_) => std::thread::sleep(std::time::Duration::from_millis(10 << attempt)),
            }
        }
        Err("剪贴板暂时不可用".into())
    }
}

impl Drop for ClipboardGuard {
    fn drop(&mut self) {
        // SAFETY: 当前线程通过本守卫成功打开剪贴板，所有发布的内存均已解锁。
        let _ = unsafe { CloseClipboard() };
    }
}

struct GlobalMemory {
    handle: HGLOBAL,
}

impl GlobalMemory {
    fn new(bytes: usize) -> Result<Self, String> {
        // SAFETY: 长度已验证；系统分配立即交给单一所有者，未发布前始终自动释放。
        let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE | GMEM_ZEROINIT, bytes) }
            .map_err(|error| format!("分配剪贴板图片失败：{error}"))?;
        Ok(Self { handle })
    }

    fn lock(&self) -> Result<GlobalLockGuard<'_>, String> {
        // SAFETY: 句柄属于本对象且尚未发布，守卫借用使内存无法在锁定期间释放。
        let ptr = unsafe { GlobalLock(self.handle) }.cast::<u8>();
        if ptr.is_null() {
            return Err("锁定剪贴板图片失败".into());
        }
        Ok(GlobalLockGuard { memory: self, ptr })
    }

    fn release(mut self) {
        self.handle = HGLOBAL::default();
    }
}

impl Drop for GlobalMemory {
    fn drop(&mut self) {
        if !self.handle.is_invalid() {
            // SAFETY: 只有未成功发布的内存才保留有效句柄，且不存在未结束的借用锁。
            let _ = unsafe { GlobalFree(Some(self.handle)) };
        }
    }
}

struct GlobalLockGuard<'a> {
    memory: &'a GlobalMemory,
    ptr: *mut u8,
}

impl Drop for GlobalLockGuard<'_> {
    fn drop(&mut self) {
        // SAFETY: 对应本守卫唯一一次成功 GlobalLock；返回 false 也可表示已完全解锁。
        let _ = unsafe { GlobalUnlock(self.memory.handle) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dib_rows_flip_vertically_convert_bgr_and_leave_padding_zero() {
        let rgba = [255, 0, 0, 255, 0, 0, 255, 255];
        let layout = DibLayout::new(1, 2, rgba.len()).unwrap();
        let mut output = vec![0; layout.pixel_bytes];
        encode_bgr_rows(1, 2, layout.stride, &rgba, &mut output);
        assert_eq!(output, [255, 0, 0, 0, 0, 0, 255, 0]);
    }

    #[test]
    fn dib_stride_handles_alignment_and_opaque_export_bytes() {
        let layout = DibLayout::new(3, 1, 12).unwrap();
        let rgba = [1, 2, 3, 255, 4, 5, 6, 255, 7, 8, 9, 255];
        let mut output = vec![0; layout.pixel_bytes];
        encode_bgr_rows(3, 1, layout.stride, &rgba, &mut output);
        assert_eq!(layout.stride, 12);
        assert_eq!(layout.total_bytes, 52);
        assert_eq!(output, [3, 2, 1, 6, 5, 4, 9, 8, 7, 0, 0, 0]);
    }

    #[test]
    fn dib_rejects_mismatched_buffers_and_excessive_dimensions_before_allocating() {
        assert!(DibLayout::new(1, 1, 3).is_err());
        assert!(DibLayout::new(0, 1, 0).is_err());
        assert!(DibLayout::new(u32::MAX, u32::MAX, usize::MAX).is_err());
        assert!(DibLayout::new(8193, 8192, 8193 * 8192 * 4).is_err());
    }
}

fn memory_bytes(bytes: &[u8]) -> Result<GlobalMemory, String> {
    let memory = GlobalMemory::new(bytes.len())?;
    {
        let lock = memory.lock()?;
        // SAFETY: 已分配并锁定 bytes.len() 字节，来源与目标不重叠。
        unsafe {
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), lock.ptr, bytes.len());
        }
    }
    Ok(memory)
}

/// 文本、文件列表和图片同次发布；粘贴目标自行选择支持的格式。
/// CF_HDROP 不限制扩展名，GIF / 视频 / 语音 / 文档等保留原文件内容。
pub fn copy_message(
    hwnd: HWND,
    text: &str,
    paths: &[std::path::PathBuf],
    image_path: Option<&std::path::Path>,
) -> Result<(), String> {
    use windows::Win32::Graphics::Gdi::{BI_BITFIELDS, BITMAPV5HEADER};
    use windows::Win32::System::DataExchange::RegisterClipboardFormatW;
    use windows::core::w;
    // SAFETY: 查询句柄有效性，不保留指针。
    if hwnd.is_invalid() || !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return Err("聊天窗口已经关闭".into());
    }
    let mut formats: Vec<(u32, GlobalMemory)> = Vec::new();
    let unicode: Vec<u8> = text
        .encode_utf16()
        .chain(std::iter::once(0))
        .flat_map(u16::to_le_bytes)
        .collect();
    formats.push((13, memory_bytes(&unicode)?));
    if !paths.is_empty() {
        let mut files = vec![0u8; 20]; // DROPFILES，pFiles=20，fWide=TRUE
        files[0..4].copy_from_slice(&20u32.to_le_bytes());
        files[16..20].copy_from_slice(&1u32.to_le_bytes());
        for path in paths {
            use std::os::windows::ffi::OsStrExt as _;
            for unit in path.as_os_str().encode_wide().chain(std::iter::once(0)) {
                files.extend_from_slice(&unit.to_le_bytes());
            }
        }
        files.extend_from_slice(&0u16.to_le_bytes());
        formats.push((15, memory_bytes(&files)?));
        // 明确请求复制，资源管理器粘贴时不应移动原附件。
        // SAFETY: 静态 UTF-16 终止字符串，系统返回注册格式 ID。
        let effect = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
        if effect != 0 {
            formats.push((effect, memory_bytes(&1u32.to_le_bytes())?));
        }
    }
    if let Some(path) = image_path {
        let mut reader = image::ImageReader::open(path)
            .map_err(|e| e.to_string())?
            .with_guessed_format()
            .map_err(|e| e.to_string())?;
        let mut limits = image::Limits::default();
        limits.max_alloc = Some(64 * 1024 * 1024);
        reader.limits(limits);
        let decoded = reader.decode().map_err(|e| format!("读取图片失败：{e}"))?;
        let rgba = decoded.to_rgba8();
        let (width, height) = rgba.dimensions();
        DibLayout::new(width, height, rgba.len())?;
        let pixels = rgba.len();
        let memory = GlobalMemory::new(size_of::<BITMAPV5HEADER>() + pixels)?;
        {
            let lock = memory.lock()?;
            let header = BITMAPV5HEADER {
                bV5Size: size_of::<BITMAPV5HEADER>() as u32,
                bV5Width: width as i32,
                bV5Height: -(height as i32),
                bV5Planes: 1,
                bV5BitCount: 32,
                bV5Compression: BI_BITFIELDS,
                bV5SizeImage: pixels as u32,
                bV5RedMask: 0x00ff0000,
                bV5GreenMask: 0x0000ff00,
                bV5BlueMask: 0x000000ff,
                bV5AlphaMask: 0xff000000,
                bV5CSType: 0x73524742,
                bV5Intent: 4,
                ..Default::default()
            };
            // SAFETY: 完整头部及像素区已分配；rgba 长度与尺寸已验证。
            unsafe {
                std::ptr::write_unaligned(lock.ptr.cast::<BITMAPV5HEADER>(), header);
                let output = std::slice::from_raw_parts_mut(
                    lock.ptr.add(size_of::<BITMAPV5HEADER>()),
                    pixels,
                );
                for (input, output) in rgba.chunks_exact(4).zip(output.chunks_exact_mut(4)) {
                    output.copy_from_slice(&[input[2], input[1], input[0], input[3]]);
                }
            }
        }
        formats.push((17, memory));
        // 旧应用使用 24 位 DIB；透明像素合成到白底。
        let layout = DibLayout::new(width, height, rgba.len())?;
        let mut legacy = vec![0u8; layout.total_bytes];
        let header = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: height as i32,
            biPlanes: 1,
            biBitCount: 24,
            biCompression: BI_RGB.0,
            biSizeImage: layout.pixel_bytes as u32,
            ..Default::default()
        };
        // SAFETY: legacy 容纳完整头部，使用未对齐写入 Vec<u8>。
        unsafe {
            std::ptr::write_unaligned(legacy.as_mut_ptr().cast::<BITMAPINFOHEADER>(), header);
        }
        let mut flattened = rgba.into_raw();
        for pixel in flattened.chunks_exact_mut(4) {
            let alpha = u16::from(pixel[3]);
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
            }
        }
        encode_bgr_rows(
            width as usize,
            height as usize,
            layout.stride,
            &flattened,
            &mut legacy[size_of::<BITMAPINFOHEADER>()..],
        );
        formats.push((CF_DIB, memory_bytes(&legacy)?));
        let mut png = std::io::Cursor::new(Vec::new());
        decoded
            .write_to(&mut png, image::ImageFormat::Png)
            .map_err(|e| e.to_string())?;
        // SAFETY: 静态 UTF-16 终止字符串；系统返回注册格式 ID。
        let png_format = unsafe { RegisterClipboardFormatW(w!("PNG")) };
        if png_format != 0 {
            formats.push((png_format, memory_bytes(png.get_ref())?));
        }
    }
    // 所有下载、编码和分配在清空前完成，准备失败不改变原剪贴板。
    let _clipboard = ClipboardGuard::open(hwnd)?;
    // SAFETY: 当前线程独占剪贴板。
    unsafe { EmptyClipboard() }.map_err(|e| format!("清空剪贴板失败：{e}"))?;
    for (format, memory) in formats {
        // SAFETY: 仅成功发布后交出已解锁的 GMEM_MOVEABLE 所有权。
        unsafe { SetClipboardData(format, Some(HANDLE(memory.handle.0))) }
            .map_err(|e| format!("复制消息失败：{e}"))?;
        memory.release();
    }
    Ok(())
}
