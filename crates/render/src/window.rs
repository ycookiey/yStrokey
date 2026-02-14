use std::mem;

use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::*;

use ystrokey_core::config::{DisplayConfig, Position};
use ystrokey_core::RenderError;

pub struct OsdWindow {
    hwnd: HWND,
    width: i32,
    height: i32,
    pub dpi: u32,
    mem_dc: HDC,
    dib_bitmap: HBITMAP,
    old_bitmap: HGDIOBJ,
}

impl OsdWindow {
    pub fn create(width: i32, height: i32, display_config: &DisplayConfig) -> Result<Self, RenderError> {
        unsafe {
            let instance = GetModuleHandleW(None)
                .map_err(|e| RenderError::CreateFailed(e.to_string()))?;

            let class_name = w!("yStrokeyOSD");
            let wc = WNDCLASSEXW {
                cbSize: mem::size_of::<WNDCLASSEXW>() as u32,
                lpfnWndProc: Some(wnd_proc),
                hInstance: HINSTANCE(instance.0),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassExW(&wc);
            // Primary monitor work area
            let (x, y) = get_primary_monitor_position(width, height, display_config);

            let ex_style = WS_EX_LAYERED
                | WS_EX_TOPMOST
                | WS_EX_TRANSPARENT
                | WS_EX_NOACTIVATE
                | WS_EX_TOOLWINDOW;

            let hwnd = CreateWindowExW(
                ex_style,
                class_name,
                w!("yStrokey"),
                WS_POPUP,
                x,
                y,
                width,
                height,
                None,
                None,
                HINSTANCE(instance.0),
                None,
            )
            .map_err(|e| RenderError::CreateFailed(e.to_string()))?;

            let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);

            let dpi = GetDpiForWindow(hwnd);
            let dpi = if dpi == 0 { 96 } else { dpi };

            // DIBセクション + メモリDC作成
            let (mem_dc, dib_bitmap, old_bitmap) = create_dib(width, height)?;

            Ok(Self {
                hwnd,
                width,
                height,
                dpi,
                mem_dc,
                dib_bitmap,
                old_bitmap,
            })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn width(&self) -> i32 {
        self.width
    }

    pub fn height(&self) -> i32 {
        self.height
    }

    /// D2D BindDC 用にメモリDCを公開
    pub fn mem_dc(&self) -> HDC {
        self.mem_dc
    }

    /// UpdateLayeredWindow で画面反映
    pub fn present(&self, opacity: f32) {
        unsafe {
            let size = SIZE {
                cx: self.width,
                cy: self.height,
            };
            let pt_src = POINT { x: 0, y: 0 };
            let blend = BLENDFUNCTION {
                BlendOp: 0,   // AC_SRC_OVER
                BlendFlags: 0,
                SourceConstantAlpha: (opacity.clamp(0.0, 1.0) * 255.0) as u8,
                AlphaFormat: 1, // AC_SRC_ALPHA
            };
            let _ = UpdateLayeredWindow(
                self.hwnd,
                None,
                None,
                Some(&size as *const SIZE),
                self.mem_dc,
                Some(&pt_src as *const POINT),
                COLORREF(0),
                Some(&blend as *const BLENDFUNCTION),
                ULW_ALPHA,
            );
        }
    }

    /// ウィンドウ矩形を取得
    pub fn get_rect(&self) -> RECT {
        unsafe {
            let mut rect = RECT::default();
            let _ = GetWindowRect(self.hwnd, &mut rect);
            rect
        }
    }

    /// WS_EX_TRANSPARENT の動的切替
    pub fn set_interactive(&self, interactive: bool) {
        unsafe {
            let style = GetWindowLongPtrW(self.hwnd, GWL_EXSTYLE);
            let new_style = if interactive {
                style & !(WS_EX_TRANSPARENT.0 as isize)
            } else {
                style | (WS_EX_TRANSPARENT.0 as isize)
            };
            if style != new_style {
                SetWindowLongPtrW(self.hwnd, GWL_EXSTYLE, new_style);
            }
        }
    }

    /// SetWindowDisplayAffinity でキャプチャ防止 (Win10 v2004+)
    pub fn set_display_affinity(&self, exclude: bool) {
        unsafe {
            let affinity = if exclude {
                WDA_EXCLUDEFROMCAPTURE
            } else {
                WDA_NONE
            };
            let _ = SetWindowDisplayAffinity(self.hwnd, affinity);
        }
    }

    pub fn set_position(&self, x: i32, y: i32) {
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                HWND::default(),
                x,
                y,
                0,
                0,
                SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }

    pub fn resize(&mut self, width: i32, height: i32) {
        unsafe {
            // 新DIBセクションを先に作成（失敗時は旧DIBを維持）
            let bmi = create_bitmapinfo(width, height);
            let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
            let new_dib = CreateDIBSection(
                HDC::default(),
                &bmi,
                DIB_RGB_COLORS,
                &mut bits,
                None,
                0,
            );
            let Ok(new_dib) = new_dib else {
                return;
            };

            // 新DIB作成成功 → 旧DIBを解放して差し替え
            self.width = width;
            self.height = height;
            SelectObject(self.mem_dc, self.old_bitmap);
            let _ = DeleteObject(self.dib_bitmap);
            self.old_bitmap = SelectObject(self.mem_dc, HGDIOBJ(new_dib.0));
            self.dib_bitmap = new_dib;

            let _ = SetWindowPos(
                self.hwnd,
                HWND::default(),
                0,
                0,
                width,
                height,
                SWP_NOMOVE | SWP_NOACTIVATE | SWP_NOZORDER,
            );
        }
    }

    pub fn update_for_dpi(&mut self, dpi: u32, suggested_rect: &RECT) {
        self.dpi = dpi;
        let new_w = suggested_rect.right - suggested_rect.left;
        let new_h = suggested_rect.bottom - suggested_rect.top;
        self.resize(new_w, new_h);
        self.set_position(suggested_rect.left, suggested_rect.top);
    }

    /// Reposition OSD to the monitor of the target window.
    /// 保存済み位置があればそれを使用し、なければDisplayConfigの設定で計算。
    pub fn reposition_to_monitor(
        &self,
        hwnd_target: HWND,
        display_config: &DisplayConfig,
    ) {
        unsafe {
            let hmon = MonitorFromWindow(hwnd_target, MONITOR_DEFAULTTONEAREST);
            let mut mi = MONITORINFOEXW::default();
            mi.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
            if GetMonitorInfoW(hmon, &mut mi as *mut _ as *mut MONITORINFO).as_bool() {
                // デバイス名取得
                let name_slice = &mi.szDevice;
                let len = name_slice
                    .iter()
                    .position(|&c| c == 0)
                    .unwrap_or(name_slice.len());
                let device_name = String::from_utf16_lossy(&name_slice[..len]);

                if let Some(&[x, y]) = display_config.monitor_positions.get(&device_name) {
                    // 保存済み位置を使用
                    self.set_position(x, y);
                } else {
                    // 設定ベースの位置計算
                    let work = mi.monitorInfo.rcWork;
                    let (x, y) = compute_position(
                        &display_config.position,
                        &work,
                        self.width,
                        self.height,
                        display_config.offset_x,
                        display_config.offset_y,
                    );
                    self.set_position(x, y);
                }
            }
        }
    }
}

impl Drop for OsdWindow {
    fn drop(&mut self) {
        unsafe {
            // 旧ビットマップ復元 → DIBセクション削除 → DC削除 → ウィンドウ破棄
            SelectObject(self.mem_dc, self.old_bitmap);
            let _ = DeleteObject(self.dib_bitmap);
            let _ = DeleteDC(self.mem_dc);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

/// 32bit ARGB top-down DIBセクションとメモリDCを作成
unsafe fn create_dib(
    width: i32,
    height: i32,
) -> Result<(HDC, HBITMAP, HGDIOBJ), RenderError> {
    let mem_dc = CreateCompatibleDC(HDC::default());

    let bmi = create_bitmapinfo(width, height);
    let mut bits: *mut std::ffi::c_void = std::ptr::null_mut();
    let dib_bitmap = CreateDIBSection(
        HDC::default(),
        &bmi,
        DIB_RGB_COLORS,
        &mut bits,
        None,
        0,
    )
    .map_err(|e| RenderError::CreateFailed(format!("CreateDIBSection: {}", e)))?;

    let old_bitmap = SelectObject(mem_dc, HGDIOBJ(dib_bitmap.0));

    Ok((mem_dc, dib_bitmap, old_bitmap))
}

fn create_bitmapinfo(width: i32, height: i32) -> BITMAPINFO {
    BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height, // top-down DIB (負値で上→下)
            biPlanes: 1,
            biBitCount: 32,
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Position enum + work area から OSD の座標を計算
fn compute_position(
    pos: &Position,
    work: &RECT,
    width: i32,
    height: i32,
    offset_x: i32,
    offset_y: i32,
) -> (i32, i32) {
    let mon_w = work.right - work.left;
    let mon_h = work.bottom - work.top;

    let (base_x, base_y) = match pos {
        Position::TopLeft => (work.left, work.top),
        Position::TopCenter => (work.left + (mon_w - width) / 2, work.top),
        Position::TopRight => (work.right - width, work.top),
        Position::BottomLeft => (work.left, work.top + mon_h - height),
        Position::BottomCenter => (work.left + (mon_w - width) / 2, work.top + mon_h - height),
        Position::BottomRight => (work.right - width, work.top + mon_h - height),
    };

    (base_x + offset_x, base_y + offset_y)
}

/// Compute initial OSD position from primary monitor work area
unsafe fn get_primary_monitor_position(width: i32, height: i32, display_config: &DisplayConfig) -> (i32, i32) {
    let pt = POINT { x: 0, y: 0 };
    let hmon = MonitorFromPoint(pt, MONITOR_DEFAULTTOPRIMARY);
    let mut mi = MONITORINFO {
        cbSize: mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    if GetMonitorInfoW(hmon, &mut mi).as_bool() {
        compute_position(
            &display_config.position,
            &mi.rcWork,
            width,
            height,
            display_config.offset_x,
            display_config.offset_y,
        )
    } else {
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);
        ((screen_w - width) / 2, screen_h - height - 48)
    }
}

/// モニタのデバイス名を取得
pub fn get_monitor_device_name(hmon: HMONITOR) -> Option<String> {
    unsafe {
        let mut mi = MONITORINFOEXW::default();
        mi.monitorInfo.cbSize = mem::size_of::<MONITORINFOEXW>() as u32;
        if GetMonitorInfoW(hmon, &mut mi as *mut _ as *mut MONITORINFO).as_bool() {
            let name_slice = &mi.szDevice;
            let len = name_slice
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(name_slice.len());
            Some(String::from_utf16_lossy(&name_slice[..len]))
        } else {
            None
        }
    }
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
