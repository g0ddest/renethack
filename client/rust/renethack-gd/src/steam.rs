//! The achievements' backends for the client: Steam behind the cargo
//! feature `steam` (the `steamworks` crate; Valve's `steam_api` library is
//! linked from it and copied next to the extension by `make steam`, never
//! committed), else the local store alone. Without Steam the game plays
//! the same.

use nh_world::achievements::{Backend, Local};

/// Steam when it runs and knows the game (`RENETHACK_STEAM_APPID`, else
/// `steam_appid.txt` in the working directory, else the game launched by
/// Steam), else the local store alone; and why not Steam, for the log.
pub fn backend() -> (Box<dyn Backend>, Option<String>) {
    #[cfg(feature = "steam")]
    {
        match imp::Steam::start() {
            Ok(steam) => (Box::new(steam), None),
            Err(e) => (Box::new(Local), Some(format!("Steam: {e}"))),
        }
    }
    #[cfg(not(feature = "steam"))]
    {
        (Box::new(Local), None)
    }
}

#[cfg(feature = "steam")]
mod imp {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use godot::prelude::*;
    use nh_world::achievements::Backend;
    use steamworks::{CallbackHandle, Client, SteamAPIInitError, UserStatsReceived};

    /// Frames an unlock waits for the user's stats before it is set anyway
    /// (Steam sends them at start).
    const PATIENCE: u32 = 600;

    pub struct Steam {
        client: Client,
        /// The user's stats have come: achievements can be set.
        ready: Arc<AtomicBool>,
        /// API names to set, oldest first.
        queue: Vec<String>,
        waited: u32,
        _received: CallbackHandle,
    }

    impl Steam {
        pub fn start() -> Result<Steam, String> {
            let app = std::env::var("RENETHACK_STEAM_APPID")
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok());
            let client = match app {
                Some(id) => Client::init_app(id),
                None => Client::init(),
            }
            .map_err(|e| {
                // the crate's words, then Steam's own
                let (SteamAPIInitError::FailedGeneric(why)
                | SteamAPIInitError::NoSteamClient(why)
                | SteamAPIInitError::VersionMismatch(why)) = &e;
                format!("{e} ({})", why.trim())
            })?;
            let ready = Arc::new(AtomicBool::new(false));
            let flag = ready.clone();
            let received = client.register_callback(move |s: UserStatsReceived| {
                if s.result.is_ok() {
                    flag.store(true, Ordering::Relaxed);
                }
            });
            Ok(Steam {
                client,
                ready,
                queue: Vec::new(),
                waited: 0,
                _received: received,
            })
        }
    }

    impl Backend for Steam {
        fn name(&self) -> &'static str {
            "steam"
        }

        fn unlock(&mut self, steam: &str) {
            self.queue.push(steam.to_string());
        }

        /// Steam's callbacks; the queued unlocks set and stored once the
        /// user's stats have come.
        fn tick(&mut self) {
            self.client.run_callbacks();
            if self.queue.is_empty() {
                return;
            }
            self.waited += 1;
            if !self.ready.load(Ordering::Relaxed) && self.waited < PATIENCE {
                return;
            }
            let stats = self.client.user_stats();
            for name in self.queue.drain(..) {
                if stats.achievement(&name).set().is_err() {
                    godot_warn!("renethack: Steam has no achievement {name}");
                }
            }
            if stats.store_stats().is_err() {
                godot_warn!("renethack: Steam did not store the achievements");
            }
        }
    }
}
