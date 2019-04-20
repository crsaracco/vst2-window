use std::sync::Arc;

use xcb;
use x11;
use log::*;

use super::gl_utils;

pub fn new_x_handle_arc() -> Arc<XHandle> {
    Arc::new(XHandle::new().unwrap())
}

pub struct XHandle {
    conn: Arc<xcb::Connection>,
    screen_num: i32,
    first_dri2_event_id: u8,
    protocols_atom: u32,
    delete_window_atom: u32,
}

impl XHandle {
    fn new() -> Result<Self, String> {
        info!("XHandle::new()");
        let (conn, screen_num) = xcb::Connection::connect_with_xlib_display().unwrap();
        let conn = Arc::new(conn);

        //conn.set_event_queue_owner(xcb::EventQueueOwner::Xcb); // TODO: need this?

        if gl_utils::glx_dec_version(conn.get_raw_dpy()) < 13 {
            return Err("glx-1.3 is not supported".into());
        }

        // Load DRI2 extensions.
        // DRI2 uses two event IDs. This function returns the first one. The next one will be the number
        // one higher (if this returns X, DRI2 uses both X and X+1).
        conn.prefetch_extension_data(xcb::dri2::id());
        let first_dri2_event_id = {
            let option_thing = conn.get_extension_data(xcb::dri2::id());
            if option_thing.is_some() {
                option_thing.unwrap().first_event()
            }
            else {
                panic!("Could not load DRI2 extension.");
            }
        };

        let wm_protocols_atom = make_cookie_atom(conn.clone(), false, "WM_PROTOCOLS");
        let wm_delete_window_atom = make_cookie_atom(conn.clone(), false, "WM_DELETE_WINDOW");

        Ok(Self {
            conn,
            screen_num,
            first_dri2_event_id,
            protocols_atom: wm_protocols_atom,
            delete_window_atom: wm_delete_window_atom,
        })
    }

    pub fn flush(&self) {
        self.conn.flush();
    }

    pub fn generate_id(&self) -> u32 {
        self.conn.generate_id()
    }

    fn conn(&self) -> Arc<xcb::Connection> {
        self.conn.clone()
    }

    pub fn conn_ref(&self) -> &xcb::Connection {
        &self.conn
    }

    pub fn screen_num(&self) -> i32 {
        self.screen_num
    }

    pub fn dri2_event_1(&self) -> u8 {
        self.first_dri2_event_id
    }

    pub fn dri2_event_2(&self) -> u8 {
        self.first_dri2_event_id + 1
    }

    pub fn delete_window_atom(&self) -> u32 {
        self.delete_window_atom
    }

    pub fn protocols_atom(&self) -> u32 {
        self.delete_window_atom
    }

    pub fn screen(&self, visual_info_screen: usize) -> xcb::base::StructPtr<xcb::ffi::xproto::xcb_screen_t> {
        let setup = self.setup();
        let screen = setup.roots().nth(visual_info_screen).unwrap();
        screen
    }

    fn setup(&self) -> xcb::StructPtr<xcb::ffi::xcb_setup_t> {
        self.conn.get_setup()
    }

    pub fn raw_display(&self) -> *mut x11::xlib::Display {
        self.conn.get_raw_dpy()
    }

    pub fn send_event(&self, window_id: u32, event: *const i8) {
        unsafe{
            xcb::ffi::xproto::xcb_send_event(
                self.conn.get_raw_conn(),
                false as u8,
                window_id,
                0,
                event,
            );
        }
    }

    pub fn wait_for_event(&self) -> Option<xcb::Event<xcb::ffi::xcb_generic_event_t>> {
        self.conn.wait_for_event()
    }
}


impl Drop for XHandle {
    fn drop(&mut self) {
        info!("XHandle::drop()");
    }
}

pub fn make_cookie_atom(conn: Arc<xcb::Connection>, only_if_exists: bool, name: &str) -> u32 {
    let cookie = xcb::intern_atom(&conn, only_if_exists, name);
    if let Ok(reply) = cookie.get_reply() {
        return reply.atom();
    }
    else {
        panic!("could not load atom for {}", name);
    }
}