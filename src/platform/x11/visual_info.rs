use std::os::raw::c_void;

use x11::glx::*;
use x11::xlib;
use log::*;

use super::gl_utils;

pub struct VisualInfo {
    glx_frame_buffer_config: *mut __GLXFBConfigRec,
    visual_info: *const xlib::XVisualInfo,
}

impl VisualInfo {
    pub fn new(display: *mut xlib::Display, screen_num: i32) -> Self {
        info!("VisualInfo::new()");
        let glx_frame_buffer_config = gl_utils::get_glxfbconfig(display, screen_num, &[
            GLX_X_RENDERABLE, 1,
            GLX_DRAWABLE_TYPE, GLX_WINDOW_BIT,
            GLX_RENDER_TYPE, GLX_RGBA_BIT,
            GLX_X_VISUAL_TYPE, GLX_TRUE_COLOR,
            GLX_RED_SIZE, 8,
            GLX_GREEN_SIZE, 8,
            GLX_BLUE_SIZE, 8,
            GLX_ALPHA_SIZE, 8,
            GLX_DEPTH_SIZE, 24,
            GLX_STENCIL_SIZE, 8,
            GLX_DOUBLEBUFFER, 1,
            0
        ]);

        let visual_info = unsafe {
            glXGetVisualFromFBConfig(display, glx_frame_buffer_config)
        };

        Self {
            glx_frame_buffer_config,
            visual_info,
        }
    }

    pub fn glx_frame_buffer_config(&self) -> *mut __GLXFBConfigRec {
        self.glx_frame_buffer_config
    }

    pub fn screen(&self) -> usize {
        unsafe {
            (*self.visual_info).screen as usize
        }
    }

    pub fn visual_id(&self) -> u32 {
        unsafe {
            (*self.visual_info).visualid as u32
        }
    }

    pub fn depth(&self) -> u8 {
        unsafe {
            (*self.visual_info).depth as u8
        }
    }
}

impl Drop for VisualInfo {
    fn drop(&mut self) {
        unsafe {
            info!("VisualInfo::drop()");
            xlib::XFree(self.visual_info as *mut c_void);
        }
    }
}