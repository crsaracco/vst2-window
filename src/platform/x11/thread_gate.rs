use std::sync::{Arc, Condvar, Mutex};

struct Internal {
    safe_to_continue: Mutex<bool>,
    condvar: Condvar,
}

pub struct Spawner {
    spawner: Arc<Internal>,
}

impl Spawner {
    pub fn wait_for_spawned(&self) {
        let mut safe = self.spawner.safe_to_continue.lock().unwrap();
        while !*safe {
            safe = self.spawner.condvar.wait(safe).unwrap();
        }
    }
}

pub struct Spawned {
    spawned: Arc<Internal>,
}

impl Spawned {
    pub fn safe_to_continue(&self) {
        let mut safe = self.spawned.safe_to_continue.lock().unwrap();
        *safe = true;
        self.spawned.condvar.notify_all();
    }
}

pub fn create_thread_gate() -> (Spawner, Spawned) {
    let internal = Arc::new(Internal {
        safe_to_continue: Mutex::new(false),
        condvar: Condvar::new(),
    });

    let spawner = Spawner {
        spawner: internal.clone(),
    };
    let spawned = Spawned { spawned: internal };

    (spawner, spawned)
}
