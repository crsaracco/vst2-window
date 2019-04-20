use std::sync::Arc;

use x11::xlib;
use log::*;

use super::x_handle;
use super::visual_info;

pub struct X11Window {
    x_handle: Arc<x_handle::XHandle>,
    visual_info: visual_info::VisualInfo,
    color_map_id: u32,
    id: u32,
}

impl X11Window {
    pub fn new(x_handle: Arc<x_handle::XHandle>, parent_id: u32, width: u16, height: u16) -> Self {
        info!("X11Window::new()");

        // Create a VisualInfo
        let visual_info = visual_info::VisualInfo::new(x_handle.raw_display(), x_handle.screen_num());

        // Get the screen struct using the VisualInfo information
        let screen = x_handle.screen(visual_info.screen());

        let mut parent = parent_id;
        if parent == 0 {
            parent = screen.root()
        }

        // Create a color map
        let color_map_id = x_handle.generate_id();
        xcb::create_colormap(x_handle.conn_ref(), xcb::COLORMAP_ALLOC_NONE as u8,color_map_id,
                             parent, visual_info.visual_id());

        // Get an ID for the window
        let id = x_handle.generate_id();

        // Some arguments to xcb::create_window
        let arguments = [
            (xcb::CW_BACK_PIXEL, screen.white_pixel()),
            (xcb::CW_BORDER_PIXEL, screen.black_pixel()),
            (xcb::CW_EVENT_MASK,
             xcb::EVENT_MASK_EXPOSURE | xcb::EVENT_MASK_BUTTON_PRESS),
            (xcb::CW_COLORMAP, color_map_id)
        ];

        // Actually create the X11 window
        xcb::create_window(x_handle.conn_ref(), visual_info.depth(), id, parent, 0, 0, width, height,
                           0, xcb::WINDOW_CLASS_INPUT_OUTPUT as u16,
                           visual_info.visual_id(), &arguments);

        // Allow deleting the window via the "protocols" / "delete_window" atoms (??? black magic)
        let protocols = [x_handle.protocols_atom()];
        xcb::change_property(x_handle.conn_ref(), xcb::PROP_MODE_REPLACE as u8,
                             id, x_handle.protocols_atom(), xcb::ATOM_ATOM, 32, &protocols);

        // Map (display) the window
        xcb::map_window(x_handle.conn_ref(), id);
        x_handle.flush();
        unsafe {
            xlib::XSync(x_handle.raw_display(), xlib::False);
        }

        Self {
            x_handle,
            visual_info,
            color_map_id,
            id,
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn visual_info(&self) -> &visual_info::VisualInfo {
        &self.visual_info
    }
}

impl Drop for X11Window {
    fn drop(&mut self) {
        info!("X11Window::Drop()");
        xcb::unmap_window(self.x_handle.conn_ref(), self.id);
        xcb::destroy_window(self.x_handle.conn_ref(), self.id);
        xcb::free_colormap(self.x_handle.conn_ref(), self.color_map_id);
        self.x_handle.flush();
    }
}