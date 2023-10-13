use std::{
    cell::{Cell, RefCell},
    collections::{HashMap, VecDeque},
};
use tokio::sync::mpsc::{Receiver, Sender};

#[allow(unused)]
#[derive(Debug, Clone)]
pub struct MediaSegment {
    pub skip: usize, // 0: not skip, 1: download but do not output, 2: totally skip
    pub is_header: bool,
    pub props: HashMap<String, String>,
    pub url: String,
}

pub struct SegmentStream {
    sequence: Cell<u64>,
    pub refresh_itvl: Cell<u64>, // in ms
    clips: RefCell<VecDeque<(MediaSegment, bool)>>,
    clip_tx: Sender<MediaSegment>, // clip, is_skip
    pub clip_rx: RefCell<Receiver<MediaSegment>>,
    refresh_tx: Sender<bool>,
    pub refresh_rx: RefCell<Receiver<bool>>,
}

impl SegmentStream {
    pub fn new() -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel(100);
        let (tx1, rx1) = tokio::sync::mpsc::channel(10);
        Self {
            sequence: Cell::new(0),
            refresh_itvl: Cell::new(1000),
            clips: RefCell::new(VecDeque::new()),
            clip_tx: tx,
            clip_rx: RefCell::new(rx),
            refresh_tx: tx1,
            refresh_rx: RefCell::new(rx1),
        }
    }

    pub async fn update_sequence(&self, sq: u64, clips: VecDeque<MediaSegment>, itvl: u64) -> anyhow::Result<()> {
        self.refresh_itvl.set(itvl);
        let mut old_clips = self.clips.borrow_mut();
        // info!("{:?}\n{:?}", &clips, &old_clips);
        let mut first_update = false;
        if old_clips.is_empty() {
            first_update = true
        }
        for (i, clip) in clips.into_iter().enumerate() {
            if self.sequence.get() < sq + i as u64 {
                self.sequence.set(sq + i as u64);
                old_clips.push_back((clip, false));
            }
        }

        while old_clips.len() > 15 {
            old_clips.pop_front();
        }

        let len = old_clips.len();
        for (i, c) in old_clips.iter_mut().enumerate() {
            if c.1 == false {
                let mut clip = c.0.clone();
                c.1 = true;
                if first_update == false {
                    self.clip_tx.send(clip).await?;
                } else {
                    if i >= len - 1 {
                        self.clip_tx.send(clip).await?;
                    } else {
                        if clip.skip == 0 {
                            clip.skip = 2;
                        }
                        self.clip_tx.send(clip).await?;
                    }
                }
            }
        }
        Ok(())
    }

    async fn refresh_task(&self) -> anyhow::Result<()> {
        let mut last_sq = 0u64;
        let mut state = 0;
        loop {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(self.refresh_itvl.get()));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await;
            loop {
                // info!(
                //     "{} {} {}",
                //     self.refresh_itvl.get(),
                //     last_sq,
                //     self.sequence.get()
                // );
                self.refresh_tx.send(true).await?;
                interval.tick().await;
                if last_sq == self.sequence.get() {
                    state = 1;
                    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                    break;
                // } else if last_sq + 1 < self.sequence.get() {
                //     last_sq = self.sequence.get();
                //     tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
                //     break;
                } else {
                    if state == 0 && self.sequence.get() != 0 {
                        state = 1;
                        break;
                    }
                    last_sq = self.sequence.get();
                }
            }
        }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        self.refresh_task().await
    }
}
