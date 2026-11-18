//! Native windows for a plug-in's own editor to live in.
//!
//! egui cannot draw a foreign editor, so on Windows the editor goes into a
//! real window: either a top-level window of its own, or a child window laid
//! over the FX Chain's right half. Both are created on the UI thread, which
//! is the thread that runs the message loop, so the plug-in's window gets
//! its messages. Other systems report that no native editor is available.
//!
//! The windows use a window class of this program. The system's "STATIC"
//! class, which an earlier version used, answers every mouse hit test with
//! "transparent", so the title bar, its buttons and the frame never received
//! a click.

use std::ffi::c_void;

#[cfg_attr(not(any(windows, bw_check)), allow(dead_code))]
pub struct NativeWindow {
    handle: *mut c_void,
    kind: Kind,
    /// The last place and visibility given to a child window, so the same
    /// placement is not sent again every frame.
    placed: std::cell::Cell<Option<(i32, i32, i32, i32, bool)>>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(not(any(windows, bw_check)), allow(dead_code))]
enum Kind {
    /// A window with a title bar; `true` when the user may resize it.
    Floating(bool),
    Child,
}

#[cfg(any(windows, bw_check))]
mod imp {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }

    pub type WndProc = unsafe extern "system" fn(*mut c_void, u32, usize, isize) -> isize;

    #[repr(C)]
    pub struct WndClassExW {
        pub size: u32,
        pub style: u32,
        pub wnd_proc: WndProc,
        pub class_extra: i32,
        pub window_extra: i32,
        pub instance: *mut c_void,
        pub icon: *mut c_void,
        pub cursor: *mut c_void,
        pub background: *mut c_void,
        pub menu_name: *const u16,
        pub class_name: *const u16,
        pub small_icon: *mut c_void,
    }

    #[cfg_attr(windows, link(name = "user32"))]
    unsafe extern "system" {
        pub fn CreateWindowExW(
            ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: *mut c_void,
            menu: *mut c_void,
            instance: *mut c_void,
            param: *mut c_void,
        ) -> *mut c_void;
        pub fn RegisterClassExW(class: *const WndClassExW) -> u16;
        pub fn DefWindowProcW(window: *mut c_void, message: u32, wparam: usize, lparam: isize) -> isize;
        pub fn LoadCursorW(instance: *mut c_void, name: *const u16) -> *mut c_void;
        pub fn DestroyWindow(window: *mut c_void) -> i32;
        pub fn IsWindow(window: *mut c_void) -> i32;
        pub fn IsIconic(window: *mut c_void) -> i32;
        pub fn ShowWindow(window: *mut c_void, command: i32) -> i32;
        pub fn AdjustWindowRectEx(rect: *mut Rect, style: u32, menu: i32, ex_style: u32) -> i32;
        pub fn GetClientRect(window: *mut c_void, rect: *mut Rect) -> i32;
        pub fn SetWindowPos(
            window: *mut c_void,
            insert_after: *mut c_void,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        pub fn SetForegroundWindow(window: *mut c_void) -> i32;
        pub fn SetFocus(window: *mut c_void) -> *mut c_void;
        pub fn GetFocus() -> *mut c_void;
        pub fn EnumThreadWindows(thread: u32, callback: unsafe extern "system" fn(*mut c_void, isize) -> i32, param: isize) -> i32;
        pub fn GetWindowTextW(window: *mut c_void, text: *mut u16, max: i32) -> i32;
        pub fn RedrawWindow(window: *mut c_void, rect: *const Rect, region: *mut c_void, flags: u32) -> i32;
        pub fn IsChild(parent: *mut c_void, window: *mut c_void) -> i32;
        #[cfg(target_pointer_width = "64")]
        pub fn GetWindowLongPtrW(window: *mut c_void, index: i32) -> isize;
        #[cfg(target_pointer_width = "64")]
        pub fn SetWindowLongPtrW(window: *mut c_void, index: i32, value: isize) -> isize;
        #[cfg(not(target_pointer_width = "64"))]
        #[link_name = "GetWindowLongW"]
        pub fn GetWindowLongPtrW(window: *mut c_void, index: i32) -> isize;
        #[cfg(not(target_pointer_width = "64"))]
        #[link_name = "SetWindowLongW"]
        pub fn SetWindowLongPtrW(window: *mut c_void, index: i32, value: isize) -> isize;
    }

    #[cfg_attr(windows, link(name = "kernel32"))]
    unsafe extern "system" {
        pub fn GetModuleHandleW(name: *const u16) -> *mut c_void;
        pub fn GetCurrentThreadId() -> u32;
    }

    pub const WS_CAPTION: u32 = 0x00C0_0000;
    pub const WS_SYSMENU: u32 = 0x0008_0000;
    pub const WS_MINIMIZEBOX: u32 = 0x0002_0000;
    pub const WS_MAXIMIZEBOX: u32 = 0x0001_0000;
    pub const WS_THICKFRAME: u32 = 0x0004_0000;
    pub const WS_VISIBLE: u32 = 0x1000_0000;
    pub const WS_CHILD: u32 = 0x4000_0000;
    pub const WS_CLIPCHILDREN: u32 = 0x0200_0000;
    pub const WS_CLIPSIBLINGS: u32 = 0x0400_0000;
    pub const SWP_NOSIZE: u32 = 0x0001;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const SWP_FRAMECHANGED: u32 = 0x0020;
    pub const SWP_SHOWWINDOW: u32 = 0x0040;
    pub const SWP_HIDEWINDOW: u32 = 0x0080;
    pub const RDW_INVALIDATE: u32 = 0x0001;
    pub const RDW_ERASE: u32 = 0x0004;
    pub const RDW_ALLCHILDREN: u32 = 0x0080;
    pub const RDW_UPDATENOW: u32 = 0x0100;
    pub const SW_RESTORE: i32 = 9;
    pub const GWL_STYLE: i32 = -16;
    pub const GWLP_HWNDPARENT: i32 = -8;
    pub const CW_USEDEFAULT: i32 = i32::MIN;
    /// COLOR_BTNFACE + 1: the brush the class paints its background with.
    pub const BACKGROUND_BRUSH: usize = 16;
    pub const IDC_ARROW: usize = 32512;
    pub const CLASS_NAME: &str = "BetterWriterPluginWindow";

    pub fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    pub fn floating_style(resizable: bool) -> u32 {
        let base = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE | WS_CLIPCHILDREN;
        if resizable { base | WS_THICKFRAME | WS_MAXIMIZEBOX } else { base }
    }

    pub const CHILD_STYLE: u32 = WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS;

    /// Registers the window class once. Its window procedure is the
    /// system's default one, which gives the window a working title bar,
    /// frame and system menu.
    pub fn register_class() -> bool {
        use std::sync::OnceLock;
        static REGISTERED: OnceLock<bool> = OnceLock::new();
        *REGISTERED.get_or_init(|| {
            let name = wide(CLASS_NAME);
            // SAFETY: plain Win32 calls with valid, NUL-terminated strings;
            // the class structure is fully initialised.
            unsafe {
                let class = WndClassExW {
                    size: std::mem::size_of::<WndClassExW>() as u32,
                    style: 0,
                    wnd_proc: DefWindowProcW,
                    class_extra: 0,
                    window_extra: 0,
                    instance: GetModuleHandleW(std::ptr::null()),
                    icon: std::ptr::null_mut(),
                    cursor: LoadCursorW(std::ptr::null_mut(), IDC_ARROW as *const u16),
                    background: BACKGROUND_BRUSH as *mut c_void,
                    menu_name: std::ptr::null(),
                    class_name: name.as_ptr(),
                    small_icon: std::ptr::null_mut(),
                };
                RegisterClassExW(&class) != 0
            }
        })
    }
}

#[cfg(any(windows, bw_check))]
impl NativeWindow {
    /// A window with a title bar whose client area is `width` x `height`
    /// pixels. When `resizable` it also gets a sizing frame and a maximise
    /// button.
    pub fn create(
        title: &str,
        width: i32,
        height: i32,
        resizable: bool,
        owner: *mut c_void,
    ) -> Result<NativeWindow, String> {
        use imp::*;
        if !register_class() {
            return Err("Windows would not register the plug-in window class".to_owned());
        }
        let style = floating_style(resizable);
        let mut frame = Rect { left: 0, top: 0, right: width, bottom: height };
        // SAFETY: plain Win32 calls with valid, NUL-terminated strings.
        unsafe {
            AdjustWindowRectEx(&mut frame, style, 0, 0);
            let class = wide(CLASS_NAME);
            let name = wide(title);
            let handle = CreateWindowExW(
                0,
                class.as_ptr(),
                name.as_ptr(),
                style,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                frame.right - frame.left,
                frame.bottom - frame.top,
                // The owner keeps the window above it.
                owner,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null_mut(),
            );
            if handle.is_null() {
                return Err("Windows would not create a window for the plug-in".to_owned());
            }
            Ok(NativeWindow {
                handle,
                kind: Kind::Floating(resizable),
                placed: std::cell::Cell::new(None),
            })
        }
    }

    /// A borderless child window of `parent` (the program's main window),
    /// created hidden; `place` puts it where it belongs and shows it.
    pub fn create_child(parent: *mut c_void, width: i32, height: i32) -> Result<NativeWindow, String> {
        use imp::*;
        if parent.is_null() {
            return Err("the program's window is not known".to_owned());
        }
        if !register_class() {
            return Err("Windows would not register the plug-in window class".to_owned());
        }
        // SAFETY: plain Win32 calls on the caller's live window. The main
        // window must clip its children, or its own drawing covers them.
        unsafe {
            let style = GetWindowLongPtrW(parent, GWL_STYLE);
            let wanted = style | (WS_CLIPCHILDREN | WS_CLIPSIBLINGS) as isize;
            if wanted != style {
                SetWindowLongPtrW(parent, GWL_STYLE, wanted);
                SetWindowPos(
                    parent,
                    std::ptr::null_mut(),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_FRAMECHANGED,
                );
            }
            let class = wide(CLASS_NAME);
            let name = wide("");
            let handle = CreateWindowExW(
                0,
                class.as_ptr(),
                name.as_ptr(),
                CHILD_STYLE & !WS_VISIBLE,
                0,
                0,
                width.max(1),
                height.max(1),
                parent,
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null_mut(),
            );
            if handle.is_null() {
                return Err("Windows would not create a child window for the plug-in".to_owned());
            }
            Ok(NativeWindow {
                handle,
                kind: Kind::Child,
                placed: std::cell::Cell::new(None),
            })
        }
    }

    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    /// False once the user closed the window.
    pub fn is_open(&self) -> bool {
        // SAFETY: `IsWindow` accepts any handle value.
        unsafe { imp::IsWindow(self.handle) != 0 }
    }

    pub fn is_resizable(&self) -> bool {
        self.kind == Kind::Floating(true)
    }

    /// The client area's size in pixels.
    pub fn client_size(&self) -> Option<(i32, i32)> {
        let mut rect = imp::Rect::default();
        // SAFETY: `rect` is a valid out-parameter.
        let found = unsafe { imp::GetClientRect(self.handle, &mut rect) } != 0;
        found.then_some((rect.right - rect.left, rect.bottom - rect.top))
    }

    /// Resizes the client area, when the plug-in asks for another size.
    pub fn resize(&self, width: i32, height: i32) {
        use imp::*;
        match self.kind {
            Kind::Floating(resizable) => {
                let mut frame = Rect { left: 0, top: 0, right: width, bottom: height };
                // SAFETY: plain Win32 calls on a window this object made.
                unsafe {
                    AdjustWindowRectEx(&mut frame, floating_style(resizable), 0, 0);
                    SetWindowPos(
                        self.handle,
                        std::ptr::null_mut(),
                        0,
                        0,
                        frame.right - frame.left,
                        frame.bottom - frame.top,
                        SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                    );
                }
            }
            Kind::Child => {
                if let Some((x, y, _, _, shown)) = self.placed.get() {
                    self.place(x, y, width, height, shown);
                }
            }
        }
    }

    /// Puts a child window at `x`, `y` (pixels in the parent's client area)
    /// with the given size, shown or hidden.
    pub fn place(&self, x: i32, y: i32, width: i32, height: i32, visible: bool) {
        use imp::*;
        let target = (x, y, width.max(1), height.max(1), visible);
        if self.placed.get() == Some(target) {
            return;
        }
        let was_visible = self.placed.get().is_some_and(|placed| placed.4);
        self.placed.set(Some(target));
        // SAFETY: plain Win32 call on a window this object made.
        unsafe {
            SetWindowPos(
                self.handle,
                std::ptr::null_mut(),
                x,
                y,
                target.2,
                target.3,
                SWP_NOZORDER
                    | SWP_NOACTIVATE
                    | if visible { SWP_SHOWWINDOW } else { SWP_HIDEWINDOW },
            );
        }
        if visible && !was_visible {
            self.redraw();
        }
    }

    /// Hides a child window where it is, without touching its size, so the
    /// editor inside is not laid out again for a size nobody sees.
    pub fn hide(&self) {
        let Some((x, y, width, height, true)) = self.placed.get() else {
            return;
        };
        self.placed.set(Some((x, y, width, height, false)));
        // SAFETY: plain Win32 call on a window this object made.
        unsafe {
            imp::SetWindowPos(
                self.handle,
                std::ptr::null_mut(),
                0,
                0,
                0,
                0,
                imp::SWP_NOMOVE
                    | imp::SWP_NOSIZE
                    | imp::SWP_NOZORDER
                    | imp::SWP_NOACTIVATE
                    | imp::SWP_HIDEWINDOW,
            );
        }
    }

    /// Paints the window and everything in it now. A plug-in editor that was
    /// hidden, or has just been made, otherwise shows a blank white area
    /// until something happens to repaint it.
    pub fn redraw(&self) {
        // SAFETY: plain Win32 call on a window this object made.
        unsafe {
            imp::RedrawWindow(
                self.handle,
                std::ptr::null(),
                std::ptr::null_mut(),
                imp::RDW_INVALIDATE | imp::RDW_ERASE | imp::RDW_ALLCHILDREN | imp::RDW_UPDATENOW,
            );
        }
    }

    /// Brings a top-level window forward, restoring it if it was minimised.
    pub fn focus(&self) {
        // SAFETY: plain Win32 calls.
        unsafe {
            if self.kind != Kind::Child && imp::IsIconic(self.handle) != 0 {
                imp::ShowWindow(self.handle, imp::SW_RESTORE);
            }
            imp::SetForegroundWindow(self.handle);
        }
    }

    /// True while the keyboard goes to this window or something inside it,
    /// which is where the plug-in's editor is.
    pub fn has_keyboard(&self) -> bool {
        // SAFETY: plain Win32 calls.
        unsafe {
            let focused = imp::GetFocus();
            !focused.is_null() && (focused == self.handle || imp::IsChild(self.handle, focused) != 0)
        }
    }
}

/// The top-level window of this thread whose title is exactly `title`, or 0.
/// Used to find the native window egui made for a viewport.
#[cfg(any(windows, bw_check))]
pub fn find_window(title: &str) -> usize {
    use imp::*;
    struct Search {
        title: Vec<u16>,
        found: usize,
    }
    unsafe extern "system" fn visit(window: *mut c_void, param: isize) -> i32 {
        // SAFETY: `param` is the address of the `Search` below, alive for the
        // whole enumeration.
        let search = unsafe { &mut *(param as *mut Search) };
        let mut buffer = [0_u16; 512];
        // SAFETY: the buffer is as long as the length given.
        let length = unsafe { GetWindowTextW(window, buffer.as_mut_ptr(), buffer.len() as i32) };
        if length > 0 && buffer[..length as usize] == search.title[..] {
            search.found = window as usize;
            return 0;
        }
        1
    }
    let mut search = Search {
        title: title.encode_utf16().collect(),
        found: 0,
    };
    // SAFETY: plain Win32 call; the callback only reads the search.
    unsafe {
        EnumThreadWindows(GetCurrentThreadId(), visit, &mut search as *mut Search as isize);
    }
    search.found
}

/// Makes `owner` the owner of the top-level window `window`, which then stays
/// above it, and goes with it when it is minimised.
#[cfg(any(windows, bw_check))]
pub fn set_owner(window: usize, owner: usize) {
    if window != 0 && owner != 0 {
        // SAFETY: plain Win32 call; stale handles just fail.
        unsafe {
            imp::SetWindowLongPtrW(window as *mut c_void, imp::GWLP_HWNDPARENT, owner as isize);
        }
    }
}

#[cfg(not(any(windows, bw_check)))]
pub fn find_window(_title: &str) -> usize {
    0
}

#[cfg(not(any(windows, bw_check)))]
pub fn set_owner(_window: usize, _owner: usize) {}

/// Gives the keyboard back to a window, for when a click lands outside the
/// plug-in's embedded editor.
#[cfg(any(windows, bw_check))]
pub fn give_keyboard_to(window: *mut c_void) {
    if !window.is_null() {
        // SAFETY: plain Win32 call; a stale handle just fails.
        unsafe {
            imp::SetFocus(window);
        }
    }
}

#[cfg(not(any(windows, bw_check)))]
pub fn give_keyboard_to(_window: *mut c_void) {}

#[cfg(not(any(windows, bw_check)))]
impl NativeWindow {
    pub fn create(
        _title: &str,
        _width: i32,
        _height: i32,
        _resizable: bool,
        _owner: *mut c_void,
    ) -> Result<NativeWindow, String> {
        Err("Plug-in editor windows are only available on Windows for now; the parameter list works everywhere".to_owned())
    }

    pub fn create_child(_parent: *mut c_void, _width: i32, _height: i32) -> Result<NativeWindow, String> {
        Err("Plug-in editors can only be shown inside the FX Chain on Windows for now; the parameter list works everywhere".to_owned())
    }

    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    pub fn is_open(&self) -> bool {
        false
    }

    pub fn is_resizable(&self) -> bool {
        false
    }

    pub fn client_size(&self) -> Option<(i32, i32)> {
        None
    }

    pub fn resize(&self, _width: i32, _height: i32) {}

    pub fn place(&self, _x: i32, _y: i32, _width: i32, _height: i32, _visible: bool) {}

    pub fn hide(&self) {}

    pub fn redraw(&self) {}

    pub fn focus(&self) {}

    pub fn has_keyboard(&self) -> bool {
        false
    }
}

#[cfg(any(windows, bw_check))]
impl Drop for NativeWindow {
    fn drop(&mut self) {
        // SAFETY: destroying a window this object made; a window the user
        // already closed makes this a harmless no-op.
        unsafe {
            if imp::IsWindow(self.handle) != 0 {
                imp::DestroyWindow(self.handle);
            }
        }
    }
}
