use std::ffi::c_void;
use log::*;

use crate::platform::PlatformWindow;

pub struct Window {
    platform_window: Box<dyn WindowImpl>,
}

impl Window {
    pub fn new(parent: *mut c_void) -> Self {
        info!("Window::new()");
        Self {
            platform_window: Box::new(PlatformWindow::new(parent)),
        }
    }
}

// TODO: Do I need to specify Drop here, or is it sufficient to just implement Drop for each WindowImpl if it needs it?
pub trait WindowImpl {
    fn new(parent: *mut c_void) -> PlatformWindow
    where Self: Sized;
}