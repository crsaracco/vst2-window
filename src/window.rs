use std::ffi::c_void;

use crate::platform::PlatformWindow;
use crate::gui_state::{GuiState, MouseEvent};

pub struct Window {
    platform_window: Box<dyn WindowImpl>,
}

impl Window {
    pub fn new(state: Box<dyn GuiState>, parent: *mut c_void, size: (u32, u32)) -> Self {
        Self {
            platform_window: Box::new(PlatformWindow::new(state, parent, size)),
        }
    }
}

// TODO: Do I need to specify Drop here, or is it sufficient to just implement Drop for each WindowImpl if it needs it?
pub trait WindowImpl {
    fn new(state: Box<dyn GuiState>, parent: *mut c_void, size: (u32, u32)) -> PlatformWindow
    where Self: Sized;
}