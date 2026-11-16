//! Frozen desktop selection on a worker thread. The overlay consumes clicks.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::ptr::null;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows_sys::Win32::Foundation::{
    HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, WAIT_FAILED, WPARAM,
};
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    MDT_EFFECTIVE_DPI,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{SetFocus, VK_ESCAPE};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

type PickResult = Result<Option<[u8; 3]>, String>;

fn cursor_position() -> Result<POINT, String> {
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: the out pointer is writable for this call.
    if unsafe { GetCursorPos(&mut point) } == 0 {
        Err(last_error("GetCursorPos"))
    } else {
        Ok(point)
    }
}

fn monitor_dpi(monitor: HMONITOR) -> u32 {
    let (mut x, mut y) = (0, 0);
    // SAFETY: monitor is returned by MonitorFromPoint and the outputs are writable.
    if unsafe { GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut x, &mut y) } >= 0 && x > 0 {
        x
    } else {
        96
    }
}

/// A top-down 32-bit cursor DIB stores BGRA with an alpha channel, while the
/// shared tool bitmap contains straight RGBA. Transparent pixels must have
/// zero RGB.
fn cursor_pixels(
    rgba: &[u8],
    [width, height]: [u16; 2],
    [hot_x, hot_y]: [u16; 2],
) -> Result<Vec<u8>, String> {
    let bytes = usize::from(width)
        .checked_mul(usize::from(height))
        .and_then(|pixels| pixels.checked_mul(4));
    if width == 0 || height == 0 || hot_x >= width || hot_y >= height || bytes != Some(rgba.len()) {
        return Err("The screen picker cursor bitmap is invalid".to_owned());
    }
    let mut bgra = Vec::with_capacity(rgba.len());
    for pixel in rgba.chunks_exact(4) {
        if let [r, g, b, a] = pixel {
            if *a == 0 {
                bgra.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                bgra.extend_from_slice(&[*b, *g, *r, *a]);
            }
        }
    }
    Ok(bgra)
}

struct CursorBitmap(HBITMAP);

impl Drop for CursorBitmap {
    fn drop(&mut self) {
        // SAFETY: these bitmaps are owned and never selected into a DC.
        if self.0 != 0 {
            unsafe { DeleteObject(self.0) };
        }
    }
}

struct OwnedCursor(HCURSOR);

type SetCursorScaling = unsafe extern "system" fn(u32) -> u32;

/// The bitmap already uses the destination monitor's physical pixels.
/// Windows 11 otherwise scales custom cursors again when they cross monitors.
/// Resolve this optional API at runtime to keep Windows 10 support.
struct CursorScaling {
    set: SetCursorScaling,
    previous: u32,
}

impl CursorScaling {
    fn unscaled() -> Option<Self> {
        static SET: std::sync::OnceLock<Option<SetCursorScaling>> = std::sync::OnceLock::new();
        let set = (*SET.get_or_init(|| {
            let name: Vec<u16> = "user32.dll".encode_utf16().chain(Some(0)).collect();
            // SAFETY: user32 is loaded for the lifetime of this desktop process.
            let module = unsafe { GetModuleHandleW(name.as_ptr()) };
            if module == 0 {
                return None;
            }
            // SAFETY: the exported function has the documented Win32 signature.
            let address = unsafe {
                GetProcAddress(module, c"SetThreadCursorCreationScaling".as_ptr().cast())
            }?;
            Some(unsafe {
                std::mem::transmute::<unsafe extern "system" fn() -> isize, SetCursorScaling>(
                    address,
                )
            })
        }))?;
        // SAFETY: only cursor creation on the current worker thread is affected.
        let previous = unsafe { set(CURSOR_CREATION_SCALING_NONE) };
        // A thread that never set a policy can report zero. Zero is not a
        // valid input to this API, so restore the explicit default instead.
        let previous = if previous == 0 {
            CURSOR_CREATION_SCALING_DEFAULT
        } else {
            previous
        };
        Some(Self { set, previous })
    }
}

impl Drop for CursorScaling {
    fn drop(&mut self) {
        // SAFETY: restore the same thread's prior cursor-creation policy.
        unsafe { (self.set)(self.previous) };
    }
}

impl OwnedCursor {
    fn pipette(dpi: u32) -> Result<Self, String> {
        use crate::ui::cursors::{bitmap, ToolCursor};
        let image = bitmap(ToolCursor::Pipette, dpi as f32 / 96.0)
            .ok_or_else(|| "The screen picker cursor could not be drawn".to_owned())?;
        Self::from_rgba(&image.rgba, image.size, image.hotspot)
    }

    fn from_rgba(rgba: &[u8], size: [u16; 2], hotspot: [u16; 2]) -> Result<Self, String> {
        let bgra = cursor_pixels(rgba, size, hotspot)?;
        let [width, height] = size.map(i32::from);
        // SAFETY: the header is initialized below; its remaining fields may be zero.
        let mut info: BITMAPINFO = unsafe { std::mem::zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Preserve the tool bitmap's top-to-bottom row order.
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..info.bmiHeader
        };
        let mut bits: *mut c_void = std::ptr::null_mut();
        // SAFETY: the validated dimensions describe exactly bgra.len() bytes.
        let color =
            CursorBitmap(unsafe { CreateDIBSection(0, &info, DIB_RGB_COLORS, &mut bits, 0, 0) });
        if color.0 == 0 || bits.is_null() {
            return Err(last_error("CreateDIBSection"));
        }
        // SAFETY: the new DIB owns this writable memory, with four bytes per pixel.
        unsafe { std::ptr::copy_nonoverlapping(bgra.as_ptr(), bits.cast::<u8>(), bgra.len()) };
        // CreateBitmap's monochrome rows are aligned to a 16-bit boundary.
        // Alpha controls the shape; an all-white AND mask preserves transparency.
        let mask_stride = usize::from(size[0]).div_ceil(16) * 2;
        let mask_bits = vec![0xff; mask_stride * usize::from(size[1])];
        // SAFETY: the buffer covers the complete word-aligned monochrome bitmap.
        let mask =
            CursorBitmap(unsafe { CreateBitmap(width, height, 1, 1, mask_bits.as_ptr().cast()) });
        if mask.0 == 0 {
            return Err(last_error("CreateBitmap"));
        }
        let icon = ICONINFO {
            fIcon: 0,
            xHotspot: u32::from(hotspot[0]),
            yHotspot: u32::from(hotspot[1]),
            hbmMask: mask.0,
            hbmColor: color.0,
        };
        // Per-monitor bitmaps must not be resized a second time by Windows.
        let _scaling = CursorScaling::unscaled();
        // SAFETY: Windows copies both source bitmaps; RAII releases them afterwards.
        let handle = unsafe { CreateIconIndirect(&icon) };
        if handle == 0 {
            Err(last_error("CreateIconIndirect"))
        } else {
            Ok(Self(handle))
        }
    }
}

impl Drop for OwnedCursor {
    fn drop(&mut self) {
        // SAFETY: the owning window class is already unregistered. A cursor still
        // selected by this thread must be replaced before its handle is destroyed.
        unsafe {
            if GetCursor() == self.0 {
                SetCursor(LoadCursorW(0, IDC_ARROW));
            }
            DestroyCursor(self.0);
        }
    }
}

fn last_error(operation: &str) -> String {
    format!("{operation}: {}", std::io::Error::last_os_error())
}

fn rgb_from_colorref(color: u32) -> Option<[u8; 3]> {
    (color != CLR_INVALID).then_some([color as u8, (color >> 8) as u8, (color >> 16) as u8])
}

fn colorref([r, g, b]: [u8; 3]) -> u32 {
    u32::from(r) | (u32::from(g) << 8) | (u32::from(b) << 16)
}

#[derive(Clone, Copy)]
struct Bounds {
    x: i32,
    y: i32,
    width: i32,
    height: i32,
}

impl Bounds {
    fn local_point(self, x: i32, y: i32) -> Option<[i32; 2]> {
        let x = x.checked_sub(self.x)?;
        let y = y.checked_sub(self.y)?;
        (x >= 0 && y >= 0 && x < self.width && y < self.height).then_some([x, y])
    }

    fn from_rect(rect: RECT) -> Option<Self> {
        let width = rect.right.checked_sub(rect.left)?;
        let height = rect.bottom.checked_sub(rect.top)?;
        (width > 0 && height > 0).then_some(Self {
            x: rect.left,
            y: rect.top,
            width,
            height,
        })
    }
}

fn hud_rect([x, y]: [i32; 2], area: Bounds) -> RECT {
    let width = 420.min(area.width).max(1);
    let height = 76.min(area.height).max(1);
    let right = area.x.saturating_add(area.width);
    let bottom = area.y.saturating_add(area.height);
    let left = if x.saturating_add(18).saturating_add(width) > right {
        x.saturating_sub(width).saturating_sub(18)
    } else {
        x.saturating_add(18)
    }
    .clamp(area.x, right.saturating_sub(width).max(area.x));
    let top = if y.saturating_add(18).saturating_add(height) > bottom {
        y.saturating_sub(height).saturating_sub(18)
    } else {
        y.saturating_add(18)
    }
    .clamp(area.y, bottom.saturating_sub(height).max(area.y));
    RECT {
        left,
        top,
        right: left.saturating_add(width),
        bottom: top.saturating_add(height),
    }
}

struct DpiContext(DPI_AWARENESS_CONTEXT);

impl DpiContext {
    fn enter() -> Result<Self, String> {
        // SAFETY: this only changes the worker thread and saves its prior context.
        let mut previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        if previous == 0 {
            // SAFETY: the older context is supported on earlier Windows 10 versions.
            previous =
                unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE) };
        }
        if previous == 0 {
            Err(last_error("SetThreadDpiAwarenessContext"))
        } else {
            Ok(Self(previous))
        }
    }
}

impl Drop for DpiContext {
    fn drop(&mut self) {
        // SAFETY: the saved context belongs to this thread.
        unsafe { SetThreadDpiAwarenessContext(self.0) };
    }
}

struct ScreenDc(HDC);

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: this DC was acquired with GetDC for the desktop.
        unsafe { ReleaseDC(0, self.0) };
    }
}

struct Snapshot {
    dc: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
    bounds: Bounds,
}

impl Snapshot {
    fn capture() -> Result<Self, String> {
        // SAFETY: GetSystemMetrics and GetDC do not retain Rust pointers.
        let (bounds, screen) = unsafe {
            (
                Bounds {
                    x: GetSystemMetrics(SM_XVIRTUALSCREEN),
                    y: GetSystemMetrics(SM_YVIRTUALSCREEN),
                    width: GetSystemMetrics(SM_CXVIRTUALSCREEN),
                    height: GetSystemMetrics(SM_CYVIRTUALSCREEN),
                },
                GetDC(0),
            )
        };
        if screen == 0 {
            return Err(last_error("GetDC"));
        }
        let screen = ScreenDc(screen);
        if bounds.width <= 0 || bounds.height <= 0 {
            return Err("The virtual desktop is empty".to_owned());
        }
        // SAFETY: the screen DC is live; the bitmap is selected into our own DC.
        let mut image = unsafe {
            Self {
                dc: CreateCompatibleDC(screen.0),
                bitmap: 0,
                previous: 0,
                bounds,
            }
        };
        if image.dc == 0 {
            return Err(last_error("CreateCompatibleDC"));
        }
        // SAFETY: dimensions are positive and Windows validates allocation sizes.
        image.bitmap = unsafe { CreateCompatibleBitmap(screen.0, bounds.width, bounds.height) };
        if image.bitmap == 0 {
            return Err(last_error("CreateCompatibleBitmap"));
        }
        // SAFETY: both GDI objects are live and exclusively owned by this thread.
        image.previous = unsafe { SelectObject(image.dc, image.bitmap) };
        if image.previous == 0 || image.previous == -1 {
            return Err(last_error("SelectObject"));
        }
        // SAFETY: the source is the complete desktop in physical coordinates.
        if unsafe {
            BitBlt(
                image.dc,
                0,
                0,
                bounds.width,
                bounds.height,
                screen.0,
                bounds.x,
                bounds.y,
                SRCCOPY | CAPTUREBLT,
            )
        } == 0
        {
            return Err(last_error("BitBlt"));
        }
        Ok(image)
    }

    fn sample(&self, point: POINT) -> PickResult {
        let Some([x, y]) = self.bounds.local_point(point.x, point.y) else {
            return Ok(None);
        };
        // SAFETY: local coordinates lie within the live, unchanged screenshot.
        rgb_from_colorref(unsafe { GetPixel(self.dc, x, y) })
            .map(Some)
            .ok_or_else(|| last_error("GetPixel"))
    }
}

impl Drop for Snapshot {
    fn drop(&mut self) {
        // SAFETY: restore the original bitmap before deleting our objects.
        unsafe {
            if self.previous != 0 && self.previous != -1 {
                SelectObject(self.dc, self.previous);
            }
            if self.bitmap != 0 {
                DeleteObject(self.bitmap);
            }
            if self.dc != 0 {
                DeleteDC(self.dc);
            }
        }
    }
}

struct WindowClass {
    name: Vec<u16>,
    instance: HINSTANCE,
    cursors: RefCell<Vec<(u32, OwnedCursor)>>,
    cursor: Cell<HCURSOR>,
    monitor: Cell<HMONITOR>,
}

impl WindowClass {
    fn register() -> Result<Self, String> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let name: Vec<u16> = format!(
            "TraceDrawScreenPick{}",
            NEXT.fetch_add(1, Ordering::Relaxed)
        )
        .encode_utf16()
        .chain(Some(0))
        .collect();
        // SAFETY: null requests this executable's module handle.
        let instance = unsafe { GetModuleHandleW(null()) };
        if instance == 0 {
            return Err(last_error("GetModuleHandleW"));
        }
        let point = cursor_position()?;
        // SAFETY: the current pointer identifies a live monitor.
        let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        let dpi = monitor_dpi(monitor);
        let cursor = OwnedCursor::pipette(dpi)?;
        let class = WNDCLASSW {
            style: 0,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: 0,
            hCursor: cursor.0,
            hbrBackground: 0,
            lpszMenuName: null(),
            lpszClassName: name.as_ptr(),
        };
        // SAFETY: the registration copies the class and its name.
        if unsafe { RegisterClassW(&class) } == 0 {
            return Err(last_error("RegisterClassW"));
        }
        let handle = cursor.0;
        Ok(Self {
            name,
            instance,
            cursors: RefCell::new(vec![(dpi, cursor)]),
            cursor: Cell::new(handle),
            monitor: Cell::new(monitor),
        })
    }

    fn apply_cursor(&self, point: POINT) -> Result<(), String> {
        // SAFETY: the point is in physical desktop coordinates.
        let monitor = unsafe { MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST) };
        if monitor != self.monitor.get() {
            let dpi = monitor_dpi(monitor);
            let mut cursors = self
                .cursors
                .try_borrow_mut()
                .map_err(|_| "The screen picker cursor is already being updated".to_owned())?;
            let handle = if let Some((_, cursor)) = cursors.iter().find(|(scale, _)| *scale == dpi)
            {
                cursor.0
            } else {
                let cursor = OwnedCursor::pipette(dpi)?;
                let handle = cursor.0;
                cursors.push((dpi, cursor));
                handle
            };
            self.cursor.set(handle);
            self.monitor.set(monitor);
        }
        // SAFETY: the class owns every cached handle until after window destruction.
        unsafe { SetCursor(self.cursor.get()) };
        Ok(())
    }
}

impl Drop for WindowClass {
    fn drop(&mut self) {
        // SAFETY: our window has been destroyed before this class is released.
        // Owned cursors are dropped only after the class no longer refers to them.
        unsafe { UnregisterClassW(self.name.as_ptr(), self.instance) };
    }
}

struct Window {
    hwnd: HWND,
    previous_foreground: HWND,
}

impl Drop for Window {
    fn drop(&mut self) {
        // SAFETY: this worker created the window. Do not steal focus after Alt+Tab.
        unsafe {
            let restore = GetForegroundWindow() == self.hwnd;
            DestroyWindow(self.hwnd);
            if restore && self.previous_foreground != 0 {
                SetForegroundWindow(self.previous_foreground);
            }
        }
    }
}

struct State {
    image: Snapshot,
    class: Rc<WindowClass>,
    hint: Vec<u16>,
    point: Cell<Option<[i32; 2]>>,
    color: Cell<Option<[u8; 3]>>,
    hud: Cell<RECT>,
    armed: Cell<bool>,
    left_down: Cell<bool>,
    result: RefCell<Option<PickResult>>,
}

impl State {
    fn finish(&self, result: PickResult) {
        if let Ok(mut slot) = self.result.try_borrow_mut() {
            if slot.is_none() {
                *slot = Some(result);
            }
        }
    }

    fn cursor(&self) -> Result<POINT, String> {
        cursor_position()
    }

    fn refresh(&self, hwnd: HWND) -> Result<(), String> {
        let point = self.cursor()?;
        self.class.apply_cursor(point)?;
        let Some(local) = self.image.bounds.local_point(point.x, point.y) else {
            return Ok(());
        };
        if self.point.get() == Some(local) {
            return Ok(());
        }
        self.color.set(self.image.sample(point)?);
        self.point.set(Some(local));
        let bounds = self.image.bounds;
        let mut area = Bounds {
            x: 0,
            y: 0,
            width: bounds.width,
            height: bounds.height,
        };
        let mut info: MONITORINFO = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        // SAFETY: monitor info is writable and uses the physical desktop coordinates.
        if unsafe { GetMonitorInfoW(MonitorFromPoint(point, MONITOR_DEFAULTTONEAREST), &mut info) }
            != 0
        {
            let rect = info.rcMonitor;
            if let (Some(x), Some(y), Some(monitor)) = (
                rect.left.checked_sub(bounds.x),
                rect.top.checked_sub(bounds.y),
                Bounds::from_rect(rect),
            ) {
                area = Bounds { x, y, ..monitor };
            }
        }
        let previous = self.hud.replace(hud_rect(local, area));
        // SAFETY: invalidate only the old and new HUD; the screenshot remains cached.
        unsafe {
            InvalidateRect(hwnd, &previous, 0);
            InvalidateRect(hwnd, &self.hud.get(), 0);
        }
        Ok(())
    }

    fn paint(&self, hwnd: HWND) {
        // SAFETY: PAINTSTRUCT contains only Win32 values initialized by BeginPaint.
        let mut paint: PAINTSTRUCT = unsafe { std::mem::zeroed() };
        // SAFETY: the hwnd belongs to this callback and the out pointer is writable.
        let dc = unsafe { BeginPaint(hwnd, &mut paint) };
        if dc == 0 {
            self.finish(Err(last_error("BeginPaint")));
            return;
        }
        let rect = paint.rcPaint;
        // SAFETY: both DCs are live. BeginPaint clips drawing to the dirty rectangles.
        let copied = unsafe {
            BitBlt(
                dc,
                rect.left,
                rect.top,
                rect.right - rect.left,
                rect.bottom - rect.top,
                self.image.dc,
                rect.left,
                rect.top,
                SRCCOPY,
            )
        };
        if copied == 0 {
            self.finish(Err(last_error("BitBlt")));
        }
        if let Some(rgb) = self.color.get() {
            let hud = self.hud.get();
            let swatch = RECT {
                left: hud.left + 12,
                top: hud.top + 10,
                right: hud.left + 42,
                bottom: hud.top + 36,
            };
            let mut hex_rect = RECT {
                left: hud.left + 54,
                top: hud.top + 12,
                right: hud.right - 10,
                bottom: hud.top + 36,
            };
            let mut hint_rect = RECT {
                left: hud.left + 12,
                top: hud.top + 44,
                right: hud.right - 12,
                bottom: hud.bottom - 10,
            };
            let [r, g, b] = rgb;
            let hex: Vec<u16> = format!("#{r:02X}{g:02X}{b:02X}").encode_utf16().collect();
            // SAFETY: stock objects are shared, never deleted; the original font is restored.
            unsafe {
                let brush = GetStockObject(DC_BRUSH);
                SetDCBrushColor(dc, colorref([30, 32, 35]));
                FillRect(dc, &hud, brush);
                SetDCBrushColor(dc, colorref(rgb));
                FillRect(dc, &swatch, brush);
                FrameRect(dc, &swatch, GetStockObject(WHITE_BRUSH));
                let font = SelectObject(dc, GetStockObject(DEFAULT_GUI_FONT));
                SetBkMode(dc, TRANSPARENT as i32);
                SetTextColor(dc, colorref([245, 245, 245]));
                DrawTextW(
                    dc,
                    hex.as_ptr(),
                    hex.len() as i32,
                    &mut hex_rect,
                    DT_SINGLELINE | DT_NOPREFIX,
                );
                DrawTextW(
                    dc,
                    self.hint.as_ptr(),
                    self.hint.len() as i32,
                    &mut hint_rect,
                    DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
                );
                if font != 0 && font != -1 {
                    SelectObject(dc, font);
                }
            }
        }
        // SAFETY: every successful BeginPaint is paired on this thread.
        unsafe { EndPaint(hwnd, &paint) };
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message == WM_NCCREATE {
        // SAFETY: Windows supplies CREATESTRUCTW for this message. The boxed State
        // remains live until after DestroyWindow returns.
        let create = unsafe { (lparam as *const CREATESTRUCTW).as_ref() };
        if let Some(create) = create {
            // `SetWindowLongPtrW` is generated as `SetWindowLongW` on 32-bit
            // Windows, so let the call infer the platform-sized integer type.
            unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, create.lpCreateParams as usize as _) };
            return 1;
        }
        return 0;
    }
    if message == WM_NCDESTROY {
        // SAFETY: stop exposing the state before the window is released.
        unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) };
    }
    // SAFETY: only WM_NCCREATE installs this pointer; state uses interior mutability
    // so synchronous, nested Win32 callbacks never alias a mutable Rust reference.
    let state = unsafe { (GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const State).as_ref() };
    if let Some(state) = state {
        match message {
            WM_PAINT => {
                state.paint(hwnd);
                return 0;
            }
            WM_ERASEBKGND => return 1,
            WM_SETCURSOR => {
                if let Err(error) = state
                    .cursor()
                    .and_then(|point| state.class.apply_cursor(point))
                {
                    state.finish(Err(error));
                }
                return 1;
            }
            WM_MOUSEMOVE => {
                if let Err(error) = state.refresh(hwnd) {
                    state.finish(Err(error));
                }
                return 0;
            }
            WM_LBUTTONDOWN => {
                state.left_down.set(true);
                return 0;
            }
            WM_LBUTTONUP => {
                if state.left_down.replace(false) {
                    state.finish(state.cursor().and_then(|point| state.image.sample(point)));
                }
                return 0;
            }
            WM_RBUTTONUP | WM_CLOSE | WM_CANCELMODE | WM_DISPLAYCHANGE => {
                state.finish(Ok(None));
                return 0;
            }
            WM_RBUTTONDOWN => return 0,
            WM_KEYDOWN | WM_SYSKEYDOWN if wparam == usize::from(VK_ESCAPE) => {
                state.finish(Ok(None));
                return 0;
            }
            WM_KILLFOCUS if state.armed.get() => {
                state.finish(Ok(None));
                return 0;
            }
            WM_ACTIVATE if state.armed.get() && (wparam & 0xffff) == WA_INACTIVE as usize => {
                state.finish(Ok(None));
                return 0;
            }
            _ => {}
        }
    }
    // SAFETY: unhandled messages retain the default Win32 behavior.
    unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
}

pub(super) fn pick(cancel: &AtomicBool, hint: &str) -> PickResult {
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    let _dpi = DpiContext::enter()?;
    let image = Snapshot::capture()?;
    let bounds = image.bounds;
    let class = Rc::new(WindowClass::register()?);
    let state = Box::new(State {
        image,
        class: Rc::clone(&class),
        hint: hint.encode_utf16().take(512).collect(),
        point: Cell::new(None),
        color: Cell::new(None),
        hud: Cell::new(RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        }),
        armed: Cell::new(false),
        left_down: Cell::new(false),
        result: RefCell::new(None),
    });
    if cancel.load(Ordering::Acquire) {
        return Ok(None);
    }
    // SAFETY: state and class stay alive beyond the window, with physical coordinates.
    let (previous_foreground, hwnd) = unsafe {
        (
            GetForegroundWindow(),
            CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class.name.as_ptr(),
                class.name.as_ptr(),
                WS_POPUP,
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                0,
                0,
                class.instance,
                (&*state as *const State).cast(),
            ),
        )
    };
    if hwnd == 0 {
        return Err(last_error("CreateWindowExW"));
    }
    let window = Window {
        hwnd,
        previous_foreground,
    };
    state.refresh(hwnd)?;
    // SAFETY: this worker owns the popup and asks for activation following the user's click.
    unsafe {
        if SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            bounds.x,
            bounds.y,
            bounds.width,
            bounds.height,
            SWP_SHOWWINDOW,
        ) == 0
        {
            return Err(last_error("SetWindowPos"));
        }
        SetForegroundWindow(hwnd);
        SetFocus(hwnd);
        UpdateWindow(hwnd);
        if GetForegroundWindow() != hwnd {
            return Err("The screen picker could not receive keyboard focus".to_owned());
        }
    }
    state.armed.set(true);
    let result = loop {
        if cancel.load(Ordering::Acquire) {
            break Ok(None);
        }
        // SAFETY: MSG is an output-only Win32 structure on this worker's own queue.
        let mut message: MSG = unsafe { std::mem::zeroed() };
        // SAFETY: PeekMessage fills a live local; dispatch uses this window's callback.
        while unsafe { PeekMessageW(&mut message, 0, 0, 0, PM_REMOVE) } != 0 {
            if message.message == WM_QUIT {
                state.finish(Ok(None));
                break;
            }
            unsafe {
                TranslateMessage(&message);
                DispatchMessageW(&message);
            }
            if state
                .result
                .try_borrow()
                .is_ok_and(|result| result.is_some())
            {
                break;
            }
        }
        if let Some(result) = state
            .result
            .try_borrow_mut()
            .ok()
            .and_then(|mut result| result.take())
        {
            break result;
        }
        // SAFETY: no handles are passed; the bounded wait keeps cancellation responsive.
        if unsafe { MsgWaitForMultipleObjectsEx(0, null(), 40, QS_ALLINPUT, MWMO_INPUTAVAILABLE) }
            == WAIT_FAILED
        {
            break Err(last_error("MsgWaitForMultipleObjectsEx"));
        }
    };
    drop(window);
    drop(state);
    drop(class);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cursor_conversion_preserves_alpha_and_bgra_channels() {
        let rgba = [255, 128, 64, 128, 25, 50, 75, 0, 20, 40, 60, 255];
        assert_eq!(
            cursor_pixels(&rgba, [3, 1], [1, 0]).expect("valid bitmap"),
            [64, 128, 255, 128, 0, 0, 0, 0, 60, 40, 20, 255]
        );
        assert!(cursor_pixels(&rgba, [3, 1], [3, 0]).is_err());
        assert!(cursor_pixels(&rgba, [3, 1], [0, 1]).is_err());
        assert!(cursor_pixels(&rgba, [0, 1], [0, 0]).is_err());
        assert!(cursor_pixels(&rgba, [4, 1], [0, 0]).is_err());
    }

    #[test]
    fn native_pipette_uses_the_shared_bitmap_size_and_hotspot_at_each_scale() {
        use crate::ui::cursors::{bitmap, ToolCursor};
        // Include returning from a 100% display to the primary 150% display.
        for dpi in [144, 96, 144, 120, 192, 96] {
            let image = bitmap(ToolCursor::Pipette, dpi as f32 / 96.0).expect("pipette bitmap");
            let cursor = OwnedCursor::pipette(dpi).expect("native pipette cursor");
            // SAFETY: GetIconInfo initializes the structure and returns owned bitmaps.
            let mut info: ICONINFO = unsafe { std::mem::zeroed() };
            assert_ne!(unsafe { GetIconInfo(cursor.0, &mut info) }, 0);
            let _mask = CursorBitmap(info.hbmMask);
            let color = CursorBitmap(info.hbmColor);
            assert_eq!(info.fIcon, 0);
            assert_eq!([info.xHotspot, info.yHotspot], image.hotspot.map(u32::from));
            // SAFETY: the color bitmap is live and the output has the required size.
            let mut native: BITMAP = unsafe { std::mem::zeroed() };
            assert_ne!(
                unsafe {
                    GetObjectW(
                        color.0,
                        std::mem::size_of::<BITMAP>() as i32,
                        (&mut native as *mut BITMAP).cast(),
                    )
                },
                0
            );
            assert_eq!([native.bmWidth, native.bmHeight], image.size.map(i32::from));
            assert_eq!(native.bmBitsPixel, 32);
        }
    }

    #[test]
    fn native_cursor_creation_disables_extra_scaling_and_restores_the_thread_policy() {
        let Some(scaling) = CursorScaling::unscaled() else {
            // Windows 10 has no cursor-creation scaling API.
            return;
        };
        let set = scaling.set;
        let previous = scaling.previous;
        // SAFETY: replace the current test thread's policy with the same value.
        assert_eq!(
            unsafe { set(CURSOR_CREATION_SCALING_NONE) },
            CURSOR_CREATION_SCALING_NONE
        );
        drop(scaling);
        // SAFETY: read back the restored policy, then restore it on exit.
        let restored = unsafe { set(CURSOR_CREATION_SCALING_NONE) };
        let _restore = CursorScaling {
            set,
            previous: restored,
        };
        assert_eq!(restored, previous);
    }

    #[test]
    fn colorref_channels_are_rgb_and_invalid_is_rejected() {
        assert_eq!(rgb_from_colorref(0x0033_2211), Some([0x11, 0x22, 0x33]));
        assert_eq!(rgb_from_colorref(CLR_INVALID), None);
        for rgb in [[0, 0, 0], [255, 255, 255], [12, 230, 45]] {
            assert_eq!(rgb_from_colorref(colorref(rgb)), Some(rgb));
        }
    }

    #[test]
    fn negative_monitor_origins_and_exclusive_edges_map_to_the_snapshot() {
        let bounds = Bounds {
            x: -1920,
            y: -240,
            width: 3840,
            height: 1440,
        };
        assert_eq!(bounds.local_point(-1920, -240), Some([0, 0]));
        assert_eq!(bounds.local_point(0, 0), Some([1920, 240]));
        assert_eq!(bounds.local_point(1919, 1199), Some([3839, 1439]));
        assert_eq!(bounds.local_point(-1921, 0), None);
        assert_eq!(bounds.local_point(1920, 0), None);
        assert_eq!(bounds.local_point(0, 1200), None);
        assert_eq!(bounds.local_point(i32::MAX, i32::MIN), None);
    }

    #[test]
    fn hud_stays_on_the_cursor_monitor_at_every_corner() {
        let area = Bounds {
            x: 1920,
            y: 240,
            width: 1280,
            height: 720,
        };
        for point in [[1920, 240], [3199, 240], [1920, 959], [3199, 959]] {
            let rect = hud_rect(point, area);
            assert!(rect.left >= 1920 && rect.right <= 3200);
            assert!(rect.top >= 240 && rect.bottom <= 960);
            assert_eq!([rect.right - rect.left, rect.bottom - rect.top], [420, 76]);
        }
        let small = hud_rect(
            [0, 0],
            Bounds {
                x: 0,
                y: 0,
                width: 100,
                height: 50,
            },
        );
        assert_eq!(
            [small.left, small.top, small.right, small.bottom],
            [0, 0, 100, 50]
        );
    }
}
