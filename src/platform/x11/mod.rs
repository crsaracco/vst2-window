// TODO: when you drop() the window, it shows a completely black screen for a split second.
// Not sure what's causing it, but I don't feel like figuring it out right now.

use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_void};
use std::sync::{Arc, Mutex};
use std::thread;
use std::ptr::null_mut;

use x11::{xlib, glx};
use log::*;

use crate::window::WindowImpl;
use crate::gui_state::{GuiState, MouseEvent};

mod gl_utils;
mod thread_gate;
mod x_handle;

pub struct PlatformWindow {
    t: Option<thread::JoinHandle<()>>,
    x_handle: Arc<x_handle::XHandle>,
    window_id_mutex: Arc<Mutex<u32>>,          // TODO: atomic?
    protocols_atom_mutex: Arc<Mutex<u32>>,     // TODO: atomic?
    delete_window_atom_mutex: Arc<Mutex<u32>>, // TODO: atomic?
}

impl WindowImpl for PlatformWindow {
    fn new(mut state: Box<dyn GuiState>, parent: *mut c_void, size: (u32, u32)) -> Self {
        info!("Window::new()");
        let mut parent_id = parent as u32;
        let (spawner, spawned) = thread_gate::create_thread_gate();

        // Create an XHandle to handle the XCB connection for us
        let x_handle = Arc::new(x_handle::XHandle::new());
        let thread_x_handle = x_handle.clone();

        // We need to get the window_id, protocols_atom, and delete_window_atom values out of the
        // spawned thread so that we can use them in our drop() function.
        let window_id_mutex = Arc::new(Mutex::new(0));
        let thread_window_id_mutex = window_id_mutex.clone();
        let protocols_atom_mutex = Arc::new(Mutex::new(0));
        let thread_protocols_atom_mutex = protocols_atom_mutex.clone();
        let delete_window_atom_mutex = Arc::new(Mutex::new(0));
        let thread_delete_window_atom_mutex = delete_window_atom_mutex.clone();

        let t = thread::spawn(move || {
            // Create visual info for the window
            #[rustfmt::skip]
            let visual_info_options = &[
                glx::GLX_X_RENDERABLE, 1,
                glx::GLX_DRAWABLE_TYPE, glx::GLX_WINDOW_BIT,
                glx::GLX_RENDER_TYPE, glx::GLX_RGBA_BIT,
                glx::GLX_X_VISUAL_TYPE, glx::GLX_TRUE_COLOR,
                glx::GLX_RED_SIZE, 8,
                glx::GLX_GREEN_SIZE, 8,
                glx::GLX_BLUE_SIZE, 8,
                glx::GLX_ALPHA_SIZE, 8,
                glx::GLX_DEPTH_SIZE, 24,
                glx::GLX_STENCIL_SIZE, 8,
                glx::GLX_DOUBLEBUFFER, 1,
                0
            ];
            let glx_frame_buffer_config = gl_utils::get_glxfbconfig(
                thread_x_handle.raw_display(),
                thread_x_handle.screen_num(),
                visual_info_options,
            );
            let visual_info = unsafe {
                glx::glXGetVisualFromFBConfig(
                    thread_x_handle.raw_display(),
                    glx_frame_buffer_config,
                )
            };
            let visual_info_id = unsafe { (*visual_info).visualid as u32 };
            let visual_info_screen = unsafe { (*visual_info).screen as usize };
            let visual_info_depth = unsafe { (*visual_info).depth as u8 };

            // Get the screen struct from the visual info for creating the colormap and window
            let screen = thread_x_handle.screen(visual_info_screen);
            if parent_id == 0 {
                parent_id = screen.root();
            }

            // Create a color map
            let color_map_id = thread_x_handle.generate_id();
            xcb::create_colormap(
                thread_x_handle.conn_ref(),
                xcb::COLORMAP_ALLOC_NONE as u8,
                color_map_id,
                parent_id,
                visual_info_id,
            );

            // Create the actual window
            #[rustfmt::skip]
            let window_options = &[
                (xcb::CW_BACK_PIXEL, screen.white_pixel()),
                (xcb::CW_BORDER_PIXEL, screen.black_pixel()),
                (xcb::CW_EVENT_MASK, xcb::EVENT_MASK_EXPOSURE | xcb::EVENT_MASK_BUTTON_PRESS),
                (xcb::CW_COLORMAP, color_map_id)
            ];
            let window_id = thread_x_handle.generate_id();
            xcb::create_window(
                thread_x_handle.conn_ref(),
                visual_info_depth,
                window_id,
                parent_id,
                0,
                0,
                size.0 as u16,
                size.1 as u16,
                0,
                xcb::WINDOW_CLASS_INPUT_OUTPUT as u16,
                visual_info_id,
                window_options,
            );
            // Put the window_id into the mutex in a scope so it's not locked forever
            {
                let mut wid = thread_window_id_mutex.lock().unwrap();
                *wid = window_id;
            }

            // Don't need this visual info anymore.
            unsafe { xlib::XFree(visual_info as *mut c_void) };

            // Allow deleting the window via the "protocols" / "delete_window" atoms (??? ...magic)
            let protocols_atom = thread_x_handle.make_cookie_atom(false, "WM_PROTOCOLS");
            let delete_window_atom = thread_x_handle.make_cookie_atom(false, "WM_DELETE_WINDOW");
            let protocols = [protocols_atom];
            xcb::change_property(
                thread_x_handle.conn_ref(),
                xcb::PROP_MODE_REPLACE as u8,
                window_id,
                protocols_atom,
                xcb::ATOM_ATOM,
                32,
                &protocols,
            );
            // Put the protocols_atom into the mutex in a scope so it's not locked forever
            {
                let mut pa = thread_protocols_atom_mutex.lock().unwrap();
                *pa = protocols_atom;
            }
            // Put the delete_window_atom into the mutex in a scope so it's not locked forever
            {
                let mut dwa = thread_delete_window_atom_mutex.lock().unwrap();
                *dwa = delete_window_atom;
            }

            // Okay, now the fun part. Make an OpenGL context!
            let gl_context = gl_utils::create_gl_context(thread_x_handle.clone(), glx_frame_buffer_config);

            // Map (display) the window.
            xcb::map_window(thread_x_handle.conn_ref(), window_id);
            thread_x_handle.flush();
            unsafe {
                xlib::XSync(thread_x_handle.raw_display(), xlib::False);
            }

            // Now we can finally let the main thread know that it's safe to continue and
            // return from new().
            spawned.safe_to_continue();

            // Handle all window events
            // We'll tell the main thread that it's safe to continue after the first "Expose" (draw) event.
            handle_events(
                thread_x_handle.clone(),
                spawned,
                window_id,
                gl_context,
                protocols_atom,
                delete_window_atom,
                state,
            );

            // After the event handler stops, it's time to destroy everything we've created.
            // Goodbye, cruel world! :(
            unsafe { glx::glXDestroyContext(thread_x_handle.raw_display(), gl_context); }
            xcb::destroy_window(thread_x_handle.conn_ref(), window_id);
            xcb::free_colormap(thread_x_handle.conn_ref(), color_map_id);
            thread_x_handle.flush();
            info!("Thread dead.");
        });

        // Wait for the thread tell us it's safe to continue
        info!("Waiting for spawned thread to finish...");
        spawner.wait_for_spawned();
        info!("Spawned thread ready. Returning from new().");

        Self {
            t: Some(t),
            x_handle,
            window_id_mutex,
            protocols_atom_mutex,
            delete_window_atom_mutex,
        }
    }
}

impl Drop for PlatformWindow {
    fn drop(&mut self) {
        info!("Window::drop()");

        // Send a delete_window CLIENT_MESSAGE event to the event handler to tell it
        // to stop processing events.
        // TODO: I just referenced some random example; I'm not sure how you're actually supposed to
        // construct these things. Any X11 experts in the house? If so... sorry about this entire
        // codebase in general.
        let window_id = *self.window_id_mutex.lock().unwrap();
        let protocols_atom = *self.protocols_atom_mutex.lock().unwrap();
        let delete_window_atom = *self.delete_window_atom_mutex.lock().unwrap();
        let mut data = [0x00u32; 5];
        data[0] = delete_window_atom;
        let message_data = xcb::ffi::xproto::xcb_client_message_data_t {
            data: unsafe { std::mem::transmute::<[u32; 5], [u8; 20]>(data) },
        };
        let delete_window_event = xcb::ffi::xproto::xcb_client_message_event_t {
            response_type: xcb::ffi::xproto::XCB_CLIENT_MESSAGE,
            format: 32,
            window: window_id,
            type_: protocols_atom,
            data: message_data,
            sequence: 0,
        };
        self.x_handle.send_event(
            window_id,
            &delete_window_event as *const xcb::ffi::xproto::xcb_client_message_event_t
                as *const i8,
        );
        self.x_handle.flush();

        // Join the thread to make sure it's dead
        if let Some(handle) = self.t.take() {
            handle.join().unwrap_or_else(|_| {
                // TODO: what do I do in this case?
                panic!("Thread join failed???");
            });
        } else {
            // TODO: what do I do in this case? probably just ignore it, I guess.
            panic!("Thread handle didn't exist anymore when attempting to join...?");
        }
    }
}
fn handle_events(
    x_handle: Arc<x_handle::XHandle>,
    gate: thread_gate::Spawned,
    window_id: u32,
    gl_context: *mut x11::glx::__GLXcontextRec,
    protocols_atom: u32,
    delete_window_atom: u32,
    mut state: Box<dyn GuiState>,
) {
    let mut first_draw = false;
    info!("Event loop begin");
    loop {
        //info!("Waiting for event");
        if let Some(ev) = x_handle.wait_for_event() {
            let ev_type = ev.response_type() & !0x80;
            match ev_type {
                xcb::EXPOSE => {
                    // X11's draw event.
                    unsafe {
                        glx::glXMakeCurrent(x_handle.raw_display(), window_id as xlib::XID, gl_context);
                    }
                    state.draw();
                    unsafe {
                        gl_utils::check_gl_error();
                        glx::glXSwapBuffers(x_handle.raw_display(), window_id as xlib::XID);
                        glx::glXMakeCurrent(x_handle.raw_display(), 0, null_mut());
                    };
                    // If this is the first time we drew the window,
                    if !first_draw {
                        first_draw = true;

                    }
                }
                xcb::BUTTON_PRESS => {
                    // X11's mouse click (down) event.
                    let button_press_event =
                        unsafe { xcb::cast_event::<xcb::ButtonPressEvent>(&ev) };

                    let x = button_press_event.event_x() as i32;
                    let y = button_press_event.event_y() as i32;
                    let mouse_button = button_press_event.detail(); // TODO: turn into enum

                    // TODO: just make a translation function somewhere else.
                    match mouse_button {
                        1 => state.handle_mouse(MouseEvent::LeftMouseButtonDown, x, y),
                        2 => state.handle_mouse(MouseEvent::MiddleMouseButtonDown, x, y),
                        3 => state.handle_mouse(MouseEvent::RightMouseButtonDown, x, y),
                        4 => state.handle_mouse(MouseEvent::ScrollUp, x, y),
                        5 => state.handle_mouse(MouseEvent::ScrollDown, x, y),
                        6 => state.handle_mouse(MouseEvent::ScrollLeft, x, y),
                        7 => state.handle_mouse(MouseEvent::ScrollRight, x, y),
                        8 => state.handle_mouse(MouseEvent::BackMouseButtonDown, x, y),
                        9 => state.handle_mouse(MouseEvent::ForwardMouseButtonDown, x, y),
                        _ => info!("Unknown mouse button: {} ({}, {})", mouse_button, x, y),
                    }

                }
                xcb::CLIENT_MESSAGE => {
                    info!("client_message");
                    let client_message_event =
                        unsafe { xcb::cast_event::<xcb::ClientMessageEvent>(&ev) };
                    if client_message_event.type_() == protocols_atom
                        && client_message_event.format() == 32
                        {
                            let protocol = client_message_event.data().data32()[0];
                            if protocol == delete_window_atom {
                                info!("delete_window message received. Killing thread!");
                                break;
                            }
                        }
                    info!("Uhh.. Some other client_message I guess.");
                }
                _ => {
                    info!("some other event");
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // run with `./run-sanitizer-tests.sh` in the root of the crate (tested on Linux).
    // No output = good.
    #[test]
    #[ignore]
    fn sanitizer_tests() {
        use std::{thread, time};
        use std::ffi::c_void;
        use std::fs::File;
        use log::*;

        // Set up a logger so we can see what's going on in the VST
        let mut logger_config = simplelog::Config::default();
        logger_config.time_format = Some("%H:%M:%S%.6f");
        simplelog::CombinedLogger::init(vec![simplelog::WriteLogger::new(
            simplelog::LevelFilter::max(),
            logger_config,
            File::create("/tmp/plugin.log").unwrap(),
        )]).unwrap();
        info!("====================================================================");

        let mut window = Some(PlatformWindow::new(0 as *mut c_void));
        thread::sleep(time::Duration::from_millis(2000));
        window = None;

    }

    #[test]
    #[ignore]
    // run with `cargo test window_open_close_no_deadlock -- --ignored`
    // Note: this test is obviously annoying: it spawns a bunch of windows over and over again,
    // effectively preventing you from doing anything else on your computer while it's running.
    // And it takes a while to run.
    // TODO: I've noticed a few times where it takes a while to draw the window, is there an issue
    // somewhere?
    fn window_open_close_no_deadlock() {
        use std::ffi::c_void;
        use std::{thread, time};
        use std::fs::File;
        use log::*;
        use rand::prelude::*;

        // Set up a logger so we can see what's going on in the test
        let mut logger_config = simplelog::Config::default();
        logger_config.time_format = Some("%H:%M:%S%.6f");
        simplelog::CombinedLogger::init(vec![simplelog::WriteLogger::new(
            simplelog::LevelFilter::max(),
            logger_config,
            File::create("/tmp/plugin.log").unwrap(),
        )]).unwrap();
        info!("====================================================================");

        let mut rng = rand::thread_rng();

        // Run a bunch of opens/closes as fast as possible
        for i in 0..1000 {
            info!("{}", i);
            let mut window = Some(PlatformWindow::new(0 as *mut c_void));
            window = None;
        }

        // Run a bunch of opens/closes with 1-10 ms delay
        for i in 0..1000 {
            info!("{}", i);
            let mut window = Some(PlatformWindow::new(0 as *mut c_void));
            thread::sleep(time::Duration::from_millis(rng.gen_range(1, 11)));
            window = None;
        }

        // Run a bunch of opens/closes with 10-100 ms delay
        for i in 0..1000 {
            info!("{}", i);
            let mut window = Some(PlatformWindow::new(0 as *mut c_void));
            thread::sleep(time::Duration::from_millis(rng.gen_range(10, 101)));
            window = None;
        }

        // Run a bunch of opens/closes with 100-1000 ms delay
        for i in 0..1000 {
            info!("{}", i);
            let mut window = Some(PlatformWindow::new(0 as *mut c_void));
            thread::sleep(time::Duration::from_millis(rng.gen_range(100, 1001)));
            window = None;
        }
    }

    #[test]
    fn it_works() {
        assert_eq!(1, 1);
    }
}