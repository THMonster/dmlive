use std::{
    collections::HashMap,
    rc::Rc,
    time::Instant,
};

use log::warn;
use tokio::sync::RwLock;

pub struct FudujiKiller {
    start_time: Instant,
    dm_stats: RwLock<HashMap<Rc<String>, (u128, i64)>>, // time and count
}

impl FudujiKiller {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            dm_stats: RwLock::new(HashMap::new()),
        }
    }

    pub async fn dm_check(&self, dm: Rc<String>) -> bool {
        let mut ret = true;
        let now = self.start_time.elapsed().as_millis();
        let mut dmst = self.dm_stats.write().await;
        match dmst.get_mut(&dm) {
            Some(it) => {
                if now > it.0 + 3000 {
                    it.0 = now;
                    it.1 = it.1.saturating_sub(1);
                } else if it.1 > 20 {
                    ret = false;
                    it.1 = it.1.saturating_add(1);
                } else {
                    it.1 = it.1.saturating_add(1);
                }
            }
            None => {
                dmst.insert(dm, (now, 1));
            }
        }
        // warn!("dm_stats len: {}", dmst.len());
        if dmst.len() > 30 {
            dmst.retain(|_, v| if (v.1 < 5) || (now > v.0 + 20_000) { false } else { true });
        }
        ret
    }
}
