extern crate log;

pub mod window;
pub mod platform;

#[cfg(test)]
mod tests {
    use super::*;

    // run with: RUSTFLAGS="-Z sanitizer=leak" cargo +nightly test --target x86_64-unknown-linux-gnu
    // (for linux)
    #[test]
    fn test_leaks() {
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
        )])
            .unwrap();
        info!("====================================================================");

        thread::sleep(time::Duration::from_millis(250));
        let mut window = Some(window::Window::new(0 as *mut c_void));
        thread::sleep(time::Duration::from_millis(250));
        window = None;
        thread::sleep(time::Duration::from_millis(250));
    }
}