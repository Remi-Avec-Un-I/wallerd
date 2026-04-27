use std::ffi::c_void;

pub enum WlEglWindow {}

#[link(name = "wayland-egl")]
unsafe extern "C" {
    pub fn wl_egl_window_create(surface: *mut c_void, width: i32, height: i32) -> *mut WlEglWindow;
    pub fn wl_egl_window_destroy(window: *mut WlEglWindow);
    pub fn wl_egl_window_resize(
        window: *mut WlEglWindow,
        width: i32,
        height: i32,
        dx: i32,
        dy: i32,
    );
}
