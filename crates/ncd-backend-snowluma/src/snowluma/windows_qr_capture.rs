// WGC 单帧只驻留内存，正常退出与取消均关闭帧池和捕获会话。
#![expect(unsafe_code, reason = "D3D 纹理与 WinRT 窗口捕获 FFI")]

use std::sync::mpsc;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFrame, Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::{Direct3D11::IDirect3DDevice, DirectXPixelFormat};
use windows::Win32::Foundation::{HMODULE, HWND};
use windows::Win32::Graphics::Direct3D::D3D_DRIVER_TYPE_HARDWARE;
use windows::Win32::Graphics::Direct3D11::*;
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_B8G8R8A8_UNORM;
use windows::Win32::Graphics::Dxgi::{DXGI_ERROR_WAS_STILL_DRAWING, IDXGIDevice};
use windows::Win32::System::WinRT::Direct3D11::{
    CreateDirect3D11DeviceFromDXGIDevice, IDirect3DDxgiInterfaceAccess,
};
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::core::{IInspectable, Interface};

pub(super) struct WindowPixels {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
}
impl Drop for WindowPixels {
    fn drop(&mut self) {
        self.bgra.fill(0);
    }
}
struct Resources {
    session: Option<GraphicsCaptureSession>,
    pool: Direct3D11CaptureFramePool,
    token: Option<i64>,
}
impl Drop for Resources {
    fn drop(&mut self) {
        if let Some(token) = self.token.take() {
            let _ = self.pool.RemoveFrameArrived(token);
        }
        if let Some(session) = self.session.take() {
            let _ = session.Close();
        }
        let _ = self.pool.Close();
    }
}
struct Frame(Direct3D11CaptureFrame);
impl Drop for Frame {
    fn drop(&mut self) {
        let _ = self.0.Close();
    }
}
struct Mapping<'a> {
    context: &'a ID3D11DeviceContext,
    texture: &'a ID3D11Texture2D,
}
impl Drop for Mapping<'_> {
    fn drop(&mut self) {
        unsafe {
            self.context.Unmap(self.texture, 0);
        }
    }
}

fn check(deadline: Instant, cancel: &CancellationToken) -> Result<(), String> {
    if cancel.is_cancelled() || Instant::now() >= deadline {
        Err("capture_cancelled".into())
    } else {
        Ok(())
    }
}

pub(super) fn capture(
    hwnd: HWND,
    deadline: Instant,
    cancel: &CancellationToken,
) -> Result<WindowPixels, String> {
    check(deadline, cancel)?;
    if !GraphicsCaptureSession::IsSupported().map_err(|_| "capture_unavailable")? {
        return Err("capture_unavailable".into());
    }
    let mut device = None;
    let mut context = None;
    // SAFETY: 输出 COM 对象归本线程持有，BGRA 格式用于单帧读回。
    unsafe {
        D3D11CreateDevice(
            None,
            D3D_DRIVER_TYPE_HARDWARE,
            HMODULE::default(),
            D3D11_CREATE_DEVICE_BGRA_SUPPORT,
            None,
            D3D11_SDK_VERSION,
            Some(&mut device),
            None,
            Some(&mut context),
        )
    }
    .map_err(|_| "capture_device_failed")?;
    let device = device.ok_or("capture_device_failed")?;
    let context = context.ok_or("capture_device_failed")?;
    let dxgi: IDXGIDevice = device.cast().map_err(|_| "capture_device_failed")?;
    let winrt: IDirect3DDevice = unsafe { CreateDirect3D11DeviceFromDXGIDevice(&dxgi) }
        .and_then(|v| v.cast())
        .map_err(|_| "capture_device_failed")?;
    let interop: IGraphicsCaptureItemInterop =
        windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()
            .map_err(|_| "capture_unavailable")?;
    // SAFETY: 调用方在捕获前后校验此 HWND 的 PID、创建时间和可见登录页。
    let item: GraphicsCaptureItem =
        unsafe { interop.CreateForWindow(hwnd) }.map_err(|_| "capture_window_failed")?;
    let size = item.Size().map_err(|_| "capture_window_failed")?;
    if size.Width <= 0
        || size.Height <= 0
        || i64::from(size.Width) * i64::from(size.Height) * 4 > 32 * 1024 * 1024
    {
        return Err("capture_size_invalid".into());
    }
    let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(
        &winrt,
        DirectXPixelFormat::B8G8R8A8UIntNormalized,
        1,
        size,
    )
    .map_err(|_| "capture_pool_failed")?;
    let mut resources = Resources {
        session: None,
        pool,
        token: None,
    };
    let (sender, receiver) = mpsc::sync_channel(1);
    let handler =
        TypedEventHandler::<Direct3D11CaptureFramePool, IInspectable>::new(move |_, _| {
            let _ = sender.try_send(());
            Ok(())
        });
    resources.token = Some(
        resources
            .pool
            .FrameArrived(&handler)
            .map_err(|_| "capture_pool_failed")?,
    );
    let session = resources
        .pool
        .CreateCaptureSession(&item)
        .map_err(|_| "capture_window_failed")?;
    resources.session = Some(session.clone());
    session.StartCapture().map_err(|_| "capture_start_failed")?;
    let frame = loop {
        check(deadline, cancel)?;
        let _ = receiver.recv_timeout(Duration::from_millis(30));
        match resources.pool.TryGetNextFrame() {
            Ok(value) => break Frame(value),
            Err(error) if error.code().0 == 0x80004003_u32 as i32 => {}
            Err(_) => return Err("capture_frame_failed".into()),
        }
    };
    let size = frame.0.ContentSize().map_err(|_| "capture_frame_failed")?;
    let width = u32::try_from(size.Width).map_err(|_| "capture_size_invalid")?;
    let height = u32::try_from(size.Height).map_err(|_| "capture_size_invalid")?;
    let count = (width as usize)
        .checked_mul(height as usize)
        .and_then(|v| v.checked_mul(4))
        .filter(|v| *v > 0 && *v <= 32 * 1024 * 1024)
        .ok_or("capture_size_invalid")?;
    let surface = frame.0.Surface().map_err(|_| "capture_frame_failed")?;
    let access: IDirect3DDxgiInterfaceAccess =
        surface.cast().map_err(|_| "capture_frame_failed")?;
    let source: ID3D11Texture2D =
        unsafe { access.GetInterface() }.map_err(|_| "capture_frame_failed")?;
    let mut desc = D3D11_TEXTURE2D_DESC::default();
    unsafe {
        source.GetDesc(&mut desc);
    }
    if width > desc.Width
        || height > desc.Height
        || desc.Format != DXGI_FORMAT_B8G8R8A8_UNORM
        || desc.SampleDesc.Count != 1
    {
        return Err("capture_layout_invalid".into());
    }
    desc.Usage = D3D11_USAGE_STAGING;
    desc.BindFlags = 0;
    desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
    desc.MiscFlags = 0;
    let mut staging = None;
    unsafe { device.CreateTexture2D(&desc, None, Some(&mut staging)) }
        .map_err(|_| "capture_frame_failed")?;
    let staging = staging.ok_or("capture_frame_failed")?;
    unsafe {
        context.CopyResource(&staging, &source);
    }
    let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
    loop {
        check(deadline, cancel)?;
        match unsafe {
            context.Map(
                &staging,
                0,
                D3D11_MAP_READ,
                D3D11_MAP_FLAG_DO_NOT_WAIT.0 as u32,
                Some(&mut mapped),
            )
        } {
            Ok(()) => break,
            Err(e) if e.code() == DXGI_ERROR_WAS_STILL_DRAWING => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(_) => return Err("capture_frame_failed".into()),
        }
    }
    let _mapping = Mapping {
        context: &context,
        texture: &staging,
    };
    let row_bytes = width as usize * 4;
    if mapped.pData.is_null() || (mapped.RowPitch as usize) < row_bytes {
        return Err("capture_layout_invalid".into());
    }
    let mut pixels = WindowPixels {
        width,
        height,
        bgra: vec![0; count],
    };
    for row in 0..height as usize {
        check(deadline, cancel)?;
        // SAFETY: 纹理行距与目标长度已校验；Map 保持到本函数退出。
        unsafe {
            std::ptr::copy_nonoverlapping(
                mapped
                    .pData
                    .cast::<u8>()
                    .add(row * mapped.RowPitch as usize),
                pixels.bgra.as_mut_ptr().add(row * row_bytes),
                row_bytes,
            );
        }
    }
    Ok(pixels)
}
