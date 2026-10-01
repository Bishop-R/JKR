//! Stock two-key held state and partial-frame accounting (cl_input.cpp:391-510).
#[derive(Clone, Copy, Default)]
pub(super) struct KeyState {
    keys: [Option<u64>; 2],
    down_at: u64,
    elapsed: u64,
    pub command_elapsed: u64,
    pub active: bool,
    pub pressed: bool,
    pub fraction: f32,
}

impl KeyState {
    pub fn event(&mut self, down: bool, key: Option<u64>, time: u64) -> bool {
        if down {
            let key = key.unwrap_or(u64::MAX);
            if self.keys.contains(&Some(key)) {
                return false;
            }
            let Some(slot) = self.keys.iter_mut().find(|slot| slot.is_none()) else {
                return false;
            };
            *slot = Some(key);
            if self.active {
                return false;
            }
            self.active = true;
            self.pressed = true;
            self.down_at = time;
            self.fraction = 1.0;
            true
        } else {
            if let Some(key) = key {
                for slot in &mut self.keys {
                    if *slot == Some(key) {
                        *slot = None;
                    }
                }
                if self.keys.iter().any(Option::is_some) {
                    return false;
                }
            } else {
                self.keys = [None; 2];
            }
            if !self.active {
                return false;
            }
            self.elapsed += time.saturating_sub(self.down_at);
            self.active = false;
            self.fraction = 0.0;
            true
        }
    }

    pub fn sample(&mut self, now: u64, millis: u64) {
        let mut elapsed = std::mem::take(&mut self.elapsed);
        if self.active {
            elapsed += now.saturating_sub(self.down_at);
            self.down_at = now;
        }
        self.fraction = (elapsed as f32 / millis.max(1) as f32).clamp(0.0, 1.0);
        self.command_elapsed += elapsed.min(millis);
    }
}
