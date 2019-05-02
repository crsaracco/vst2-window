use x11;
use xcb;
use log::*;

pub struct XHandle {
    conn: xcb::Connection,
    screen_num: i32,
}

impl XHandle {
    pub fn new() -> Self {
        info!("XHandle::new()");

        let (conn, screen_num) = xcb::Connection::connect_with_xlib_display().unwrap();

        Self { conn, screen_num }
    }

    pub fn screen_num(&self) -> i32 {
        self.screen_num
    }

    pub fn raw_display(&self) -> *mut x11::xlib::Display {
        self.conn.get_raw_dpy()
    }

    pub fn screen(
        &self,
        visual_info_screen: usize,
    ) -> xcb::base::StructPtr<xcb::ffi::xproto::xcb_screen_t> {
        let setup = self.conn.get_setup();
        let screen = setup.roots().nth(visual_info_screen).unwrap();
        screen
    }

    pub fn generate_id(&self) -> u32 {
        self.conn.generate_id()
    }

    pub fn conn_ref(&self) -> &xcb::Connection {
        &self.conn
    }

    pub fn make_cookie_atom(&self, only_if_exists: bool, name: &str) -> u32 {
        let cookie = xcb::intern_atom(&self.conn, only_if_exists, name);
        if let Ok(reply) = cookie.get_reply() {
            return reply.atom();
        } else {
            panic!("could not load atom for {}", name);
        }
    }

    pub fn flush(&self) {
        self.conn.flush();
    }

    pub fn wait_for_event(&self) -> Option<xcb::Event<xcb::ffi::xcb_generic_event_t>> {
        self.conn.wait_for_event()
    }

    pub fn send_event(&self, window_id: u32, event: *const i8) {
        unsafe {
            xcb::ffi::xproto::xcb_send_event(
                self.conn.get_raw_conn(),
                false as u8,
                window_id,
                0,
                event,
            );
        }
    }
}

impl Drop for XHandle {
    fn drop(&mut self) {
        info!("XHandle::drop()");
    }
}
