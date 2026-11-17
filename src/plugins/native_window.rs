//! A bare top-level window for a plug-in's own editor to live in.
//!
//! egui cannot embed foreign child windows, so on Windows the editor opens in
//! a small window of its own, created on the UI thread (which is the thread
//! that runs the message loop, so the plug-in's window gets its messages).
//! Other systems report that no native editor is available.

use std::ffi::c_void;

pub struct NativeWindow {
    handle: *mut c_void,
}

#[cfg(any(windows, bw_check))]
mod imp {
    use std::ffi::c_void;

    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
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
        pub fn DestroyWindow(window: *mut c_void) -> i32;
        pub fn IsWindow(window: *mut c_void) -> i32;
        pub fn AdjustWindowRectEx(rect: *mut Rect, style: u32, menu: i32, ex_style: u32) -> i32;
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
    }

    pub const WS_CAPTION: u32 = 0x00C0_0000;
    pub const WS_SYSMENU: u32 = 0x0008_0000;
    pub const WS_MINIMIZEBOX: u32 = 0x0002_0000;
    pub const WS_VISIBLE: u32 = 0x1000_0000;
    pub const WS_CLIPCHILDREN: u32 = 0x0200_0000;
    pub const SWP_NOMOVE: u32 = 0x0002;
    pub const SWP_NOZORDER: u32 = 0x0004;
    pub const SWP_NOACTIVATE: u32 = 0x0010;
    pub const CW_USEDEFAULT: i32 = i32::MIN;
    pub const STYLE: u32 = WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX | WS_VISIBLE | WS_CLIPCHILDREN;

    pub fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

#[cfg(any(windows, bw_check))]
impl NativeWindow {
    /// A window whose client area is `width` x `height` pixels. It uses the
    /// system's "STATIC" window class, which paints nothing of its own and
    /// closes itself when its close button is pressed.
    pub fn create(title: &str, width: i32, height: i32) -> Result<NativeWindow, String> {
        use imp::*;
        let mut frame = Rect { left: 0, top: 0, right: width, bottom: height };
        // SAFETY: plain Win32 calls with valid, NUL-terminated strings.
        unsafe {
            AdjustWindowRectEx(&mut frame, STYLE, 0, 0);
            let class = wide("STATIC");
            let name = wide(title);
            let handle = CreateWindowExW(
                0,
                class.as_ptr(),
                name.as_ptr(),
                STYLE,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                frame.right - frame.left,
                frame.bottom - frame.top,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if handle.is_null() {
                return Err("Windows would not create a window for the plug-in".to_owned());
            }
            Ok(NativeWindow { handle })
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

    /// Resizes the client area, when the plug-in asks for another size.
    pub fn resize(&self, width: i32, height: i32) {
        use imp::*;
        let mut frame = Rect { left: 0, top: 0, right: width, bottom: height };
        // SAFETY: plain Win32 calls on a window this object made.
        unsafe {
            AdjustWindowRectEx(&mut frame, STYLE, 0, 0);
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

    pub fn focus(&self) {
        // SAFETY: plain Win32 call.
        unsafe {
            imp::SetForegroundWindow(self.handle);
        }
    }
}

#[cfg(not(any(windows, bw_check)))]
impl NativeWindow {
    pub fn create(_title: &str, _width: i32, _height: i32) -> Result<NativeWindow, String> {
        Err("Plug-in editor windows are only available on Windows for now; the parameter list below works everywhere".to_owned())
    }

    pub fn handle(&self) -> *mut c_void {
        self.handle
    }

    pub fn is_open(&self) -> bool {
        false
    }

    pub fn resize(&self, _width: i32, _height: i32) {}

    pub fn focus(&self) {}
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
