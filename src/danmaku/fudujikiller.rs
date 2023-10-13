use std::{
    cell::RefCell,
    collections::{hash_map::DefaultHasher, HashMap},
    hash::{Hash, Hasher},
    time::Instant,
};

pub struct FudujiKiller {
    start_time: Instant,
    dm_stats: RefCell<HashMap<u64, (u128, u64)>>, // time and count
}

impl FudujiKiller {
    pub fn new() -> Self {
        Self {
            start_time: Instant::now(),
            dm_stats: RefCell::new(HashMap::new()),
        }
    }

    pub fn dm_check(&self, dm: &str) -> bool {
        let mut s = DefaultHasher::new();
        dm.hash(&mut s);
        let dm_hash = s.finish();
        let mut ret = true;
        let now = self.start_time.elapsed().as_millis();
        let mut dmst = self.dm_stats.borrow_mut();
        match dmst.get_mut(&dm_hash) {
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
                dmst.insert(dm_hash, (now, 1));
            }
        }
        // warn!("dm_stats len: {}", dmst.len());
        if dmst.len() > 30 {
            dmst.retain(|_, v| !((v.1 < 5) || (now > v.0 + 20_000)));
        }
        ret
    }
}
