// TODO: This file is pretty dang messy. Clean up.

use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_void};
use std::sync::Arc;
use std::ptr::null_mut;

use x11::{xlib, glx};

use super::x_handle;
use super::x11_window;

type GlXCreateContextAttribsARBProc =
unsafe extern "C" fn (dpy: *mut xlib::Display, fbc: glx::GLXFBConfig,
                      share_context: glx::GLXContext, direct: xlib::Bool,
                      attribs: *const c_int) -> glx::GLXContext;

pub unsafe fn check_gl_error() {
    let err = gl::GetError();
    if err != gl::NO_ERROR {
        println!("got gl error {}", err);
    }
}

const GLX_CONTEXT_MAJOR_VERSION_ARB: u32 = 0x2091;
const GLX_CONTEXT_MINOR_VERSION_ARB: u32 = 0x2092;

pub fn glx_dec_version(dpy: *mut xlib::Display) -> i32 {
    let mut maj: c_int = 0;
    let mut min: c_int = 0;
    unsafe {
        if glx::glXQueryVersion(dpy, &mut maj as *mut c_int, &mut min as *mut c_int) == 0 {
            panic!("cannot get glx version");
        }
    }
    (maj*10 + min) as i32
}

pub fn get_glxfbconfig(dpy: *mut xlib::Display, screen_num: i32, visual_attribs: &[i32]) -> glx::GLXFBConfig {
    unsafe {
        let mut fbcount: c_int = 0;
        let fbcs = glx::glXChooseFBConfig(dpy, screen_num, visual_attribs.as_ptr(), &mut fbcount as *mut c_int);

        if fbcount == 0 {
            panic!("could not find compatible fb config");
        }

        // Pick the first from the list
        let fbc = *fbcs;
        xlib::XFree(fbcs as *mut c_void);
        fbc
    }
}

static mut GL_CONTEXT_ERROR_OCCURRED: bool = false;
unsafe extern "C" fn gl_context_error_handler(
    _dpy: *mut xlib::Display,
    _ev: *mut xlib::XErrorEvent) -> i32 {
    GL_CONTEXT_ERROR_OCCURRED = true;
    0
}

pub fn create_gl_context(x_handle: Arc<x_handle::XHandle>, window: &x11_window::X11Window) -> *mut x11::glx::__GLXcontextRec {
    // Load GL extensions
    let glx_exts = unsafe {
        CStr::from_ptr(
            glx::glXQueryExtensionsString(x_handle.raw_display(), x_handle.screen_num()))
            .to_str().unwrap()
    };

    // We need at least the GLX_ARB_create_context extension to continue.
    if !check_glx_extension(&glx_exts, "GLX_ARB_create_context") {
        panic!("could not find GLX extension GLX_ARB_create_context");
    }

    // We have to load the "glXCreateContextAttribsARB" GL function differently, for some reason. (???)
    let glx_create_context_attribs: GlXCreateContextAttribsARBProc = unsafe {
        std::mem::transmute(load_gl_func("glXCreateContextAttribsARB"))
    };

    // Now we can load all of the other GL functions.
    unsafe {
        gl::load_with(|n| load_gl_func(&n));
    }

    // We need to ensure that this function is loaded, or else we don't have OpenGL 3 support.
    if !gl::GenVertexArrays::is_loaded() {
        panic!("no GL3 support available!");
    }

    // Install a context error handler
    unsafe {
        GL_CONTEXT_ERROR_OCCURRED = false;
    }
    let old_handler = unsafe {
        xlib::XSetErrorHandler(Some(gl_context_error_handler))
    };

    // Create the context attributes
    let context_attributes: [c_int; 5] = [
        GLX_CONTEXT_MAJOR_VERSION_ARB as c_int, 3,
        GLX_CONTEXT_MINOR_VERSION_ARB as c_int, 0,
        0
    ];

    // And finally, create the context itself
    let ctx = unsafe {
        glx_create_context_attribs(x_handle.raw_display(), window.visual_info().glx_frame_buffer_config(), null_mut(),
                                   xlib::True, &context_attributes[0] as *const c_int)
    };

    x_handle.flush();
    unsafe {
        xlib::XSync(x_handle.raw_display(), xlib::False);
        xlib::XSetErrorHandler(std::mem::transmute(old_handler));
    }

    unsafe {
        if ctx.is_null() || GL_CONTEXT_ERROR_OCCURRED {
            panic!("error when creating gl-3.0 context");
        }

        if glx::glXIsDirect(x_handle.raw_display(), ctx) == 0 {
            panic!("obtained indirect rendering context")
        }
    }

    ctx
}

fn check_glx_extension(glx_exts: &str, ext_name: &str) -> bool {
    for glx_ext in glx_exts.split(" ") {
        if glx_ext == ext_name {
            return true;
        }
    }
    false
}

unsafe fn load_gl_func(name: &str) -> *mut c_void {
    let cname = CString::new(name).unwrap();
    let ptr: *mut c_void = std::mem::transmute(glx::glXGetProcAddress(
        cname.as_ptr() as *const u8
    ));
    if ptr.is_null() {
        panic!("could not load {}", name);
    }
    ptr
}