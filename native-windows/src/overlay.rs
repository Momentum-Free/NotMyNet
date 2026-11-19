//! Read-only shell integration. Only our own layered popup is created or repositioned.
use crate::{
    OverlayEdge, OverlayMonitor, OverlaySettings,
    overlay_config::{slice_boundary, strip_rect},
};
use std::{
    io,
    ptr::{null, null_mut},
    sync::OnceLock,
};
use windows_sys::{
    Win32::{
        Foundation::{ERROR_SUCCESS, HWND, LPARAM, LRESULT, RECT, WPARAM},
        Graphics::Dwm::{DWMWA_CLOAKED, DwmGetWindowAttribute},
        Graphics::Gdi::{
            BeginPaint, DC_BRUSH, EndPaint, EnumDisplayMonitors, FillRect, GetMonitorInfoW,
            GetStockObject, HDC, HMONITOR, InvalidateRect, MONITOR_DEFAULTTONULL, MONITORINFOEXW,
            MonitorFromWindow, PAINTSTRUCT, SetDCBrushColor,
        },
        System::{
            LibraryLoader::GetModuleHandleW,
            Registry::{HKEY_CURRENT_USER, RRF_RT_REG_DWORD, RegGetValueW},
        },
        UI::{
            HiDpi::{
                DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
                SetThreadDpiAwarenessContext,
            },
            WindowsAndMessaging::*,
        },
    },
    core::w,
};

const CLASS: *const u16 = w!("NotMyNet.ProbeStrip.1");

struct PhysicalPixels(DPI_AWARENESS_CONTEXT);
impl PhysicalPixels {
    fn enter() -> Option<Self> {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        (!previous.is_null()).then_some(Self(previous))
    }
}
impl Drop for PhysicalPixels {
    fn drop(&mut self) {
        unsafe {
            SetThreadDpiAwarenessContext(self.0);
        }
    }
}

struct Screen {
    handle: HMONITOR,
    info: MONITORINFOEXW,
}
fn monitor_list() -> Vec<Screen> {
    unsafe extern "system" fn collect(h: HMONITOR, _: HDC, _: *mut RECT, data: LPARAM) -> i32 {
        // SAFETY: synchronous enumeration borrows this live Vec exclusively through LPARAM.
        let list = unsafe { &mut *(data as *mut Vec<Screen>) };
        let mut info = MONITORINFOEXW::default();
        info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
        if unsafe { GetMonitorInfoW(h, &mut info.monitorInfo) } != 0 {
            list.push(Screen { handle: h, info });
        }
        1
    }
    let mut list = Vec::new();
    unsafe {
        EnumDisplayMonitors(
            null_mut(),
            null(),
            Some(collect),
            (&mut list as *mut Vec<Screen>) as LPARAM,
        );
    }
    list
}

pub fn screens() -> Vec<(OverlayMonitor, String)> {
    let mut options = vec![(OverlayMonitor::default(), "Primary display".into())];
    let Some(_dpi) = PhysicalPixels::enter() else {
        return options;
    };
    for screen in monitor_list() {
        // The primary shortcut follows Windows' primary setting; list specific displays separately.
        let id = OverlayMonitor(screen.info.szDevice);
        if id.is_primary() {
            continue;
        }
        let label =
            String::from_utf16_lossy(&id.0[..id.0.iter().position(|&c| c == 0).unwrap_or(32)]);
        let label = label.strip_prefix(r"\\.\").unwrap_or(&label);
        let r = screen.info.monitorInfo.rcMonitor;
        options.push((
            id,
            format!("{label} ({} × {})", r.right - r.left, r.bottom - r.top),
        ));
    }
    options
}

pub fn transparency_enabled() -> Option<bool> {
    // This is only a hint about the Windows preference, not a test of taskbar compositing.
    let mut value = 0u32;
    let mut size = size_of_val(&value) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            w!(r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize"),
            w!("EnableTransparency"),
            RRF_RT_REG_DWORD,
            null_mut(),
            (&mut value as *mut u32).cast(),
            &mut size,
        )
    };
    (status == ERROR_SUCCESS && size == size_of_val(&value) as u32).then_some(value != 0)
}

#[derive(Default)]
struct Paint {
    colors: Vec<u32>,
    vertical: bool,
}

#[derive(Default)]
pub struct Overlay {
    hwnd: HWND,
    // A raw owning pointer shared with our same-thread paint callback. No borrow
    // of the allocation is kept across a Win32 call.
    paint: *mut Paint,
    placement: Option<([i32; 4], HWND)>,
}

impl Overlay {
    pub fn hide(&mut self) {
        if self.owns_window() {
            unsafe {
                ShowWindow(self.hwnd, SW_HIDE);
            }
        }
        self.placement = None;
    }

    pub fn update(&mut self, config: OverlaySettings, colors: &[u32]) -> Result<(), String> {
        if !config.enabled || colors.is_empty() {
            self.hide();
            return Ok(());
        }
        let result = self.show(config, colors);
        if result.is_err() {
            self.hide();
        }
        result
    }

    fn show(&mut self, config: OverlaySettings, colors: &[u32]) -> Result<(), String> {
        let _dpi =
            PhysicalPixels::enter().ok_or("Could not read the display's physical pixels.")?;
        let screen = monitor_list()
            .into_iter()
            .find(|s| {
                if config.monitor.is_primary() {
                    s.info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0
                } else {
                    OverlayMonitor(s.info.szDevice) == config.monitor
                }
            })
            .ok_or("Overlay display is disconnected. Choose another display in Settings.")?;
        let info = screen.info.monitorInfo;
        let vertical = matches!(config.edge, OverlayEdge::Left | OverlayEdge::Right);
        let (rect, behind) = if config.behind_taskbar {
            if transparency_enabled() != Some(true) {
                return Err(
                    "Behind-taskbar mode needs Windows transparency effects enabled.".into(),
                );
            }
            let (bar, bounds) = taskbar(&screen, config.edge)
                .filter(|(bar, _)| is_topmost(*bar))
                .ok_or("No usable taskbar on this display edge. Choose its edge or turn off behind-taskbar mode.")?;
            // Use the taskbar's full background instead of the edge-strip thickness.
            (bounds, bar)
        } else {
            (
                strip_rect(into_rect(info.rcWork), config.edge, config.thickness_px)
                    .ok_or("Could not place the overlay on this display.")?,
                null_mut(),
            )
        };
        if !self.owns_window() {
            self.close();
            self.create()?;
        }
        let moved = self.placement != Some((rect, behind));
        // SAFETY: create installed this allocation, and it is only read or mutated on this thread.
        let repaint = unsafe {
            let data = &mut *self.paint;
            let changed = data.vertical != vertical || data.colors != colors;
            if changed {
                data.vertical = vertical;
                data.colors.clear();
                data.colors.extend_from_slice(colors);
            }
            changed
        };
        if moved || (!behind.is_null() && (!is_topmost(behind) || !follows(self.hwnd, behind))) {
            self.hide();
            let [x, y, right, bottom] = rect;
            if !behind.is_null() && !is_topmost(behind) {
                return Err("Could not place the overlay safely behind the taskbar.".into());
            }
            let after = if behind.is_null() {
                HWND_TOPMOST
            } else {
                behind
            };
            let ok = unsafe {
                SetWindowPos(
                    self.hwnd,
                    after,
                    x,
                    y,
                    right - x,
                    bottom - y,
                    SWP_NOACTIVATE | SWP_NOOWNERZORDER,
                )
            };
            if ok == 0
                || (!behind.is_null()
                    && (taskbar(&screen, config.edge) != Some((behind, rect))
                        || !is_topmost(behind)
                        || !follows(self.hwnd, behind)))
            {
                return Err("Could not position the overlay.".into());
            }
            if unsafe {
                SetWindowPos(
                    self.hwnd,
                    null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_SHOWWINDOW,
                )
            } == 0
            {
                return Err("Could not show the overlay.".into());
            }
            self.placement = Some((rect, behind));
        }
        if moved || repaint {
            unsafe {
                InvalidateRect(self.hwnd, null(), 0);
            }
        }
        Ok(())
    }

    fn create(&mut self) -> Result<(), String> {
        let module = unsafe { GetModuleHandleW(null()) };
        if module.is_null() {
            return Err("Could not initialize the overlay.".into());
        }
        static REGISTERED: OnceLock<bool> = OnceLock::new();
        if !*REGISTERED.get_or_init(|| unsafe {
            RegisterClassW(&WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                hInstance: module,
                lpszClassName: CLASS,
                ..Default::default()
            }) != 0
        }) {
            return Err("Could not register the overlay window.".into());
        }
        // Layered + transparent passes mouse input to other apps. Tool window avoids Alt+Tab;
        // NOACTIVATE and SWP_NOACTIVATE additionally prevent stealing the current app's focus.
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_NOACTIVATE
                    | WS_EX_TRANSPARENT
                    | WS_EX_LAYERED
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TOPMOST,
                CLASS,
                w!("NotMyNet probe overlay"),
                WS_POPUP,
                0,
                0,
                1,
                1,
                null_mut(),
                null_mut(),
                module,
                null(),
            )
        };
        if hwnd.is_null() {
            return Err(format!(
                "Could not create the overlay: {}",
                io::Error::last_os_error()
            ));
        }
        if unsafe { SetLayeredWindowAttributes(hwnd, 0, 255, LWA_ALPHA) } == 0 {
            unsafe {
                DestroyWindow(hwnd);
            }
            return Err("Could not initialize the click-through overlay.".into());
        }
        let paint = Box::into_raw(Box::new(Paint::default()));
        unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, paint as isize);
        }
        self.hwnd = hwnd;
        self.paint = paint;
        if !self.owns_window() {
            unsafe {
                DestroyWindow(hwnd);
            }
            self.close();
            return Err("Could not bind the overlay painter.".into());
        }
        Ok(())
    }

    fn owns_window(&self) -> bool {
        !self.hwnd.is_null()
            && !self.paint.is_null()
            && unsafe { GetWindowLongPtrW(self.hwnd, GWLP_USERDATA) } == self.paint as isize
    }

    fn close(&mut self) {
        if self.owns_window() {
            // Detach the borrowed pointer before destroying our own HWND.
            unsafe {
                SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                DestroyWindow(self.hwnd);
            }
        }
        self.hwnd = null_mut();
        self.placement = None;
        if !self.paint.is_null() {
            unsafe {
                drop(Box::from_raw(self.paint));
            }
            self.paint = null_mut();
        }
    }
}

impl Drop for Overlay {
    fn drop(&mut self) {
        self.close();
    }
}

fn into_rect(r: RECT) -> [i32; 4] {
    [r.left, r.top, r.right, r.bottom]
}
fn is_topmost(hwnd: HWND) -> bool {
    unsafe { GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & WS_EX_TOPMOST as isize != 0 }
}
fn follows(window: HWND, other: HWND) -> bool {
    // Look only for an existing z-order relationship, without altering any other HWND.
    let mut cursor = window;
    for _ in 0..4096 {
        cursor = unsafe { GetWindow(cursor, GW_HWNDPREV) };
        if cursor.is_null() {
            break;
        }
        if cursor == other {
            return true;
        }
    }
    false
}

fn taskbar_is_visible(bar: HWND) -> bool {
    let mut cloaked = 0u32;
    // A shell window can retain WS_VISIBLE while DWM is not displaying it.
    unsafe {
        IsWindowVisible(bar) != 0
            && DwmGetWindowAttribute(
                bar,
                DWMWA_CLOAKED as u32,
                (&mut cloaked as *mut u32).cast(),
                size_of_val(&cloaked) as u32,
            ) >= 0
            && cloaked == 0
    }
}

fn taskbar(screen: &Screen, edge: OverlayEdge) -> Option<(HWND, [i32; 4])> {
    let mut shell_pid = 0;
    unsafe {
        GetWindowThreadProcessId(GetShellWindow(), &mut shell_pid);
    }
    if shell_pid == 0 {
        return None;
    }
    let [left, top, right, bottom] = into_rect(screen.info.monitorInfo.rcMonitor);
    for class in [w!("Shell_TrayWnd"), w!("Shell_SecondaryTrayWnd")] {
        let mut bar = null_mut();
        loop {
            bar = unsafe { FindWindowExW(null_mut(), bar, class, null()) };
            if bar.is_null() {
                break;
            }
            let mut pid = 0;
            unsafe {
                GetWindowThreadProcessId(bar, &mut pid);
            }
            if pid != shell_pid
                || !taskbar_is_visible(bar)
                || unsafe { MonitorFromWindow(bar, MONITOR_DEFAULTTONULL) } != screen.handle
            {
                continue;
            }
            let mut r = RECT::default();
            if unsafe { GetWindowRect(bar, &mut r) } == 0 {
                continue;
            }
            let [l, t, r, b] = [
                left.max(r.left),
                top.max(r.top),
                right.min(r.right),
                bottom.min(r.bottom),
            ];
            if r - l <= 2 || b - t <= 2 {
                continue;
            } // hidden auto-hide taskbar not usable
            let horizontal = r - l >= b - t;
            let matches = match edge {
                OverlayEdge::Top => horizontal && t <= top + 2,
                OverlayEdge::Bottom => horizontal && b >= bottom - 2,
                OverlayEdge::Left => !horizontal && l <= left + 2,
                OverlayEdge::Right => !horizontal && r >= right - 2,
            };
            if matches {
                return Some((bar, [l, t, r, b]));
            }
        }
    }
    None
}

unsafe extern "system" fn window_proc(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    match message {
        WM_NCDESTROY => unsafe {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            DefWindowProcW(hwnd, message, w, l)
        },
        WM_NCHITTEST => HTTRANSPARENT as LRESULT,
        WM_MOUSEACTIVATE => MA_NOACTIVATE as LRESULT,
        WM_ERASEBKGND => 1,
        WM_PAINT => {
            unsafe {
                let mut paint = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut paint);
                let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const Paint;
                let mut client = RECT::default();
                if !dc.is_null() && !ptr.is_null() && GetClientRect(hwnd, &mut client) != 0 {
                    draw(dc, &*ptr, client.right, client.bottom);
                }
                EndPaint(hwnd, &paint);
            }
            0
        }
        _ => unsafe { DefWindowProcW(hwnd, message, w, l) },
    }
}

unsafe fn draw(dc: HDC, data: &Paint, width: i32, height: i32) {
    if width <= 0 || height <= 0 || data.colors.is_empty() {
        return;
    }
    let length = if data.vertical { height } else { width };
    let brush = unsafe { GetStockObject(DC_BRUSH) };
    for (i, color) in data.colors.iter().copied().enumerate() {
        let a = slice_boundary(i, data.colors.len(), length);
        let b = slice_boundary(i + 1, data.colors.len(), length);
        if a == b {
            continue;
        }
        let r = if data.vertical {
            RECT {
                left: 0,
                top: a,
                right: width,
                bottom: b,
            }
        } else {
            RECT {
                left: a,
                top: 0,
                right: b,
                bottom: height,
            }
        };
        let colorref = ((color & 0xff0000) >> 16) | (color & 0x00ff00) | ((color & 0x0000ff) << 16);
        unsafe {
            SetDCBrushColor(dc, colorref);
            FillRect(dc, &r, brush);
        }
    }
}
