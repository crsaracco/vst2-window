use std::os::raw::c_int;
use x11::{xlib, glx};
use std::os::raw::c_void;

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
