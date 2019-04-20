// TODO: make open() wait until the window is fully initialized (condvar?)
// TODO: leaking Arc<x_handle::XHandle> ????
use std::os::raw::c_void;
use std::thread;
use std::sync::Arc;
use std::sync::atomic;
use std::ptr::null_mut;

use x11::glx::*;
use x11::xlib;

use log::*;

use crate::window::WindowImpl;

mod x_handle;
mod gl_utils;
mod visual_info;
mod x11_window;

pub struct PlatformWindow {
    t: Option<thread::JoinHandle<()>>,
    window_id: Arc<atomic::AtomicU32>,
    x_handle: Arc<x_handle::XHandle>,
}

impl WindowImpl for PlatformWindow {
    fn new(parent: *mut c_void) -> Self {
        info!("PlatformWindow::new()");
        let parent_id = parent as u32;

        let x_handle = x_handle::new_x_handle_arc();
        let thread_x_handle = x_handle.clone();

        let window_id = Arc::new(atomic::AtomicU32::new(0));
        let thread_window_id = window_id.clone();

        let t = thread::spawn(move || {
            let window = x11_window::X11Window::new(thread_x_handle.clone(), parent_id, 1024, 1024);
            thread_window_id.store(window.id(), atomic::Ordering::Relaxed);
            info!("Window id: {}", window.id());

            let ctx = gl_utils::create_gl_context(thread_x_handle.clone(), &window);

            // Event handling:
            handle_events(
                thread_x_handle,
                window.id(),
                ctx,
            );
        });

        Self {
            t: Some(t),
            window_id,
            x_handle,
        }
    }
}

impl Drop for PlatformWindow {
    fn drop(&mut self) {
        info!("PlatformWindow::drop()");
        // Send a CLIENT_MESSAGE event to our event handler to tell it to stop processing events
        unsafe{
            let window_id = self.window_id.load(atomic::Ordering::Relaxed);
            info!("window_id: {}", window_id);

            let d = xcb::ffi::xproto::xcb_client_message_data_t { data: [0x00u8; 20] };

            let ev = xcb::ffi::xproto::xcb_client_message_event_t {
                response_type: xcb::ffi::xproto::XCB_CLIENT_MESSAGE,
                format: 32,
                window: window_id,
                type_: self.x_handle.delete_window_atom(),
                data: d,
                sequence: 0,
            };

            self.x_handle.send_event(window_id, &ev as *const xcb::ffi::xproto::xcb_client_message_event_t as *const i8);


            self.x_handle.flush();
        }

        info!("joining....");

        if let Some(handle) = self.t.take() {
            handle.join();
        }
        else {
            info!("FAILED TO JOIN????");
        }

        info!("Arc count: {}", Arc::strong_count(&self.x_handle));

        info!("platform window dropped.");
    }
}

fn handle_events(
    x_handle: Arc<x_handle::XHandle>,
    win: u32,
    ctx: *mut x11::glx::__GLXcontextRec,
) {
    loop {
        info!("Event loop begin");
        if let Some(ev) = x_handle.wait_for_event() {
            info!("fucking event type: {:?}", ev.response_type());
            let ev_type = ev.response_type() & !0x80;
            match ev_type {
                xcb::EXPOSE => {
                    unsafe {
                        glXMakeCurrent(x_handle.raw_display(), win as xlib::XID, ctx);
                        gl::ClearColor(0.5f32, 0.5f32, 1.0f32, 1.0f32);
                        gl::Clear(gl::COLOR_BUFFER_BIT);
                        gl::Flush();
                        gl_utils::check_gl_error();
                        glXSwapBuffers(x_handle.raw_display(), win as xlib::XID);
                        glXMakeCurrent(x_handle.raw_display(), 0, null_mut());
                    }
                },
                xcb::BUTTON_PRESS => {
                    info!("Click!");
                },
                xcb::CLIENT_MESSAGE => {
                    info!("Client message");
                    let cmev = unsafe {
                        xcb::cast_event::<xcb::ClientMessageEvent>(&ev)
                    };
                    if cmev.type_() == x_handle.protocols_atom() && cmev.format() == 32 {
                        let protocol = cmev.data().data32()[0];
                        if protocol == x_handle.delete_window_atom() {
                            info!("THREAD STOP!!");
                            break;
                        }
                    }

                    // TODO(hack): Move this somewhere else.
                    unsafe {
                        glXDestroyContext(x_handle.conn().get_raw_dpy(), ctx);
                    }

                    break;
                },
                _ => {
                    // the following stuff is not obvious at all, but it's necessary
                    // to handle GL when XCB owns the event queue.
                    if ev_type == x_handle.dri2_event_1() || ev_type == x_handle.dri2_event_2() {
                        // these are libgl dri2 event that need special handling
                        // see https://bugs.freedesktop.org/show_bug.cgi?id=35945#c4
                        // and mailing thread starting here:
                        // http://lists.freedesktop.org/archives/xcb/2015-November/010556.html
                        unsafe {
                            if let Some(proc_) =
                            xlib::XESetWireToEvent(x_handle.raw_display(),
                                                   ev_type as i32, None) {
                                xlib::XESetWireToEvent(x_handle.raw_display(),
                                                       ev_type as i32, Some(proc_));
                                let raw_ev = ev.ptr;
                                (*raw_ev).sequence =
                                    xlib::XLastKnownRequestProcessed(
                                        x_handle.raw_display()) as u16;
                                let mut dummy: xlib::XEvent = std::mem::zeroed();
                                proc_(x_handle.raw_display(),
                                      &mut dummy as *mut xlib::XEvent,
                                      raw_ev as *mut xlib::xEvent);
                            }
                        }
                    }
                }
            }
        }
        else {
            break;
        }
        info!("Event loop end");
    }
    x_handle.flush();
    info!("Thread dead.");
}