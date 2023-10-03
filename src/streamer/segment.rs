use log::info;
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
};
use tokio::sync::mpsc::Sender;

pub struct SegmentStream {
    sequence: Cell<u64>,
    clips: RefCell<VecDeque<(String, bool)>>,
    clip_tx: Sender<String>,
}

impl SegmentStream {
    pub fn new(clip_tx: Sender<String>) -> Self {
        Self {
            sequence: Cell::new(0),
            clips: RefCell::new(VecDeque::new()),
            clip_tx,
        }
    }

    pub async fn update_sequence(&self, _sq: u64, clips: VecDeque<String>) -> anyhow::Result<()> {
        let mut old_clips = self.clips.borrow_mut();
        // warn!("{:?}\n{:?}", &clips, &old_clips);
        let mut first_update = false;
        if old_clips.is_empty() {
            first_update = true
        }
        for clip in clips.into_iter() {
            if old_clips.iter().find(|&x| x.0.eq(&clip)).is_none() {
                self.sequence.set(self.sequence.get().saturating_add(1));
                old_clips.push_back((clip, false));
            }
        }

        while old_clips.len() > 15 {
            old_clips.pop_front();
        }

        let len = old_clips.len();
        for (i, c) in old_clips.iter_mut().enumerate() {
            if c.1 == false {
                c.1 = true;
                if first_update == false {
                    self.clip_tx.send(c.0.to_string()).await?;
                } else {
                    if i >= len - 1 {
                        self.clip_tx.send(c.0.to_string()).await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn refresh_task(&self, refresh_tx: Sender<bool>) -> anyhow::Result<()> {
        let mut itvl = 1000;
        let mut last_sq = 0u64;
        loop {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(itvl));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                // info!(
                //     "refresh interval: {} {} {}",
                //     itvl,
                //     last_sq,
                //     self.sequence.get()
                // );
                refresh_tx.send(true).await?;
                interval.tick().await;
                if last_sq == self.sequence.get() {
                    itvl = itvl.saturating_add(100);
                    break;
                } else if last_sq + 1 < self.sequence.get() {
                    last_sq = self.sequence.get();
                    itvl = itvl.saturating_sub(200);
                    break;
                } else {
                    last_sq = self.sequence.get();
                }
            }
        }
    }

    pub async fn run(&self, refresh_tx: Sender<bool>) -> anyhow::Result<()> {
        self.refresh_task(refresh_tx).await
    }
}
