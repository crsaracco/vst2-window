// TODO: kill thread correctly (wm_delete_window?)
// TODO: make open() wait until the window is fully initialized (condvar?)

use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_void};
use std::ptr::null_mut;
use std::thread;
use std::sync::Arc;
use std::sync::atomic;

use x11::glx::*;
use x11::xlib;

use log::*;

use crate::window::WindowImpl;

mod x_handle;
mod gl_utils;
mod visual_info;

// Random stuff that should be refactored elsewhere probably

type GlXCreateContextAttribsARBProc =
unsafe extern "C" fn (dpy: *mut xlib::Display, fbc: GLXFBConfig,
                      share_context: GLXContext, direct: xlib::Bool,
                      attribs: *const c_int) -> GLXContext;

static mut ctx_error_occurred: bool = false;
unsafe extern "C" fn ctx_error_handler(
    _dpy: *mut xlib::Display,
    _ev: *mut xlib::XErrorEvent) -> i32 {
    ctx_error_occurred = true;
    0
}

fn check_glx_extension(glx_exts: &str, ext_name: &str) -> bool {
    for glx_ext in glx_exts.split(" ") {
        if glx_ext == ext_name {
            return true;
        }
    }
    false
}

unsafe fn load_gl_func (name: &str) -> *mut c_void {
    let cname = CString::new(name).unwrap();
    let ptr: *mut c_void = std::mem::transmute(glXGetProcAddress(
        cname.as_ptr() as *const u8
    ));
    if ptr.is_null() {
        panic!("could not load {}", name);
    }
    ptr
}

unsafe fn check_gl_error() {
    let err = gl::GetError();
    if err != gl::NO_ERROR {
        println!("got gl error {}", err);
    }
}

const GLX_CONTEXT_MAJOR_VERSION_ARB: u32 = 0x2091;
const GLX_CONTEXT_MINOR_VERSION_ARB: u32 = 0x2092;

// End random stuff



pub struct PlatformWindow {
    t: Option<thread::JoinHandle<()>>,
    window_id: Arc<atomic::AtomicU32>,
    x_handle: Arc<x_handle::XHandle>,
}

impl WindowImpl for PlatformWindow {
    fn new(parent: *mut c_void) -> Self {
        let parent_id = parent as u32;

        let x_handle = x_handle::XHandle::new_arc().unwrap();
        let thread_x_handle = x_handle.clone();

        let window_id = Arc::new(atomic::AtomicU32::new(0));
        let thread_window_id = window_id.clone();

        let t = thread::spawn(move || {
            let visual_info = visual_info::VisualInfo::new(thread_x_handle.raw_display(), thread_x_handle.screen_num());
            let screen = thread_x_handle.screen(visual_info.screen());

            let cmap = thread_x_handle.generate_id();
            let win = thread_x_handle.generate_id();
            thread_window_id.store(win, atomic::Ordering::Relaxed);
            info!("Window id: {}", win);

            xcb::create_colormap(&thread_x_handle.conn(), xcb::COLORMAP_ALLOC_NONE as u8,
                                 cmap, parent_id, visual_info.visual_id());

            let cw_values = [
                (xcb::CW_BACK_PIXEL, screen.white_pixel()),
                (xcb::CW_BORDER_PIXEL, screen.black_pixel()),
                (xcb::CW_EVENT_MASK,
                 xcb::EVENT_MASK_EXPOSURE | xcb::EVENT_MASK_BUTTON_PRESS),
                (xcb::CW_COLORMAP, cmap)
            ];

            xcb::create_window(&thread_x_handle.conn(), visual_info.depth(), win, parent_id, 0, 0, 1024, 1024,
                               0, xcb::WINDOW_CLASS_INPUT_OUTPUT as u16,
                               visual_info.visual_id(), &cw_values);

            // TODO: drop visual_info here.

            let title = "XCB OpenGL";
            xcb::change_property(&thread_x_handle.conn(),
                                 xcb::PROP_MODE_REPLACE as u8,
                                 win,
                                 xcb::ATOM_WM_NAME,
                                 xcb::ATOM_STRING,
                                 8, title.as_bytes());

            let protocols = [thread_x_handle.protocols_atom()];
            xcb::change_property(&thread_x_handle.conn(), xcb::PROP_MODE_REPLACE as u8,
                                 win, thread_x_handle.protocols_atom(), xcb::ATOM_ATOM, 32, &protocols);

            xcb::map_window(&thread_x_handle.conn(), win);
            thread_x_handle.flush();
            unsafe {
                xlib::XSync(thread_x_handle.raw_display(), xlib::False);
            }

            let glx_exts = unsafe {
                CStr::from_ptr(
                    glXQueryExtensionsString(thread_x_handle.raw_display(), thread_x_handle.screen_num()))
                    .to_str().unwrap()
            };

            if !check_glx_extension(&glx_exts, "GLX_ARB_create_context") {
                panic!("could not find GLX extension GLX_ARB_create_context");
            }

            // with glx, no need of a current context is needed to load symbols
            // otherwise we would need to create a temporary legacy GL context
            // for loading symbols (at least glXCreateContextAttribsARB)
            let glx_create_context_attribs: GlXCreateContextAttribsARBProc = unsafe {
                std::mem::transmute(load_gl_func("glXCreateContextAttribsARB"))
            };

            // loading all other symbols
            unsafe {
                gl::load_with(|n| load_gl_func(&n));
            }

            if !gl::GenVertexArrays::is_loaded() {
                panic!("no GL3 support available!");
            }

            // installing an event handler to check if error is generated
            unsafe {
                ctx_error_occurred = false;
            }
            let old_handler = unsafe {
                xlib::XSetErrorHandler(Some(ctx_error_handler))
            };

            let context_attribs: [c_int; 5] = [
                GLX_CONTEXT_MAJOR_VERSION_ARB as c_int, 3,
                GLX_CONTEXT_MINOR_VERSION_ARB as c_int, 0,
                0
            ];
            let ctx = unsafe {
                glx_create_context_attribs(thread_x_handle.raw_display(), visual_info.glx_frame_buffer_config(), null_mut(),
                                           xlib::True, &context_attribs[0] as *const c_int)
            };

            // TODO: or maybe drop VisualInfo here?

            thread_x_handle.flush();
            unsafe {
                xlib::XSync(thread_x_handle.raw_display(), xlib::False);
                xlib::XSetErrorHandler(std::mem::transmute(old_handler));
            }

            unsafe {
                if ctx.is_null() || ctx_error_occurred {
                    panic!("error when creating gl-3.0 context");
                }

                if glXIsDirect(thread_x_handle.raw_display(), ctx) == 0 {
                    panic!("obtained indirect rendering context")
                }
            }

            // Event handling:
            handle_events(
                thread_x_handle,
                win,
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

            xcb::ffi::xproto::xcb_send_event(
                self.x_handle.conn().get_raw_conn(),
                false as u8,
                window_id,
                0,
                &ev as *const xcb::ffi::xproto::xcb_client_message_event_t as *const i8
            );

            self.x_handle.flush();
        }

        info!("joining....");

        if let Some(handle) = self.t.take() {
            handle.join();
        }

        info!("dropped.");
    }
}

fn handle_events(
    x_handle: Arc<x_handle::XHandle>,
    win: u32,
    ctx: *mut x11::glx::__GLXcontextRec,
) {
    loop {
        info!("Event loop begin");
        if let Some(ev) = x_handle.conn().wait_for_event() {
            info!("fucking event type: {:?}", ev.response_type());
            let ev_type = ev.response_type() & !0x80;
            match ev_type {
                xcb::EXPOSE => {
                    unsafe {
                        glXMakeCurrent(x_handle.raw_display(), win as xlib::XID, ctx);
                        gl::ClearColor(0.5f32, 0.5f32, 1.0f32, 1.0f32);
                        gl::Clear(gl::COLOR_BUFFER_BIT);
                        gl::Flush();
                        check_gl_error();
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