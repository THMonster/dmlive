pub mod flv;
pub mod hls;
pub mod youtube;

use crate::{
    config::{ConfigManager, StreamType},
    dmlive::DMLMessage,
};
use anyhow::*;
use std::{ops::Deref, sync::Arc};

pub struct Streamer {
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
}

impl Streamer {
    pub fn new(
        cm: Arc<ConfigManager>,
        im: Arc<crate::ipcmanager::IPCManager>,
        mtx: async_channel::Sender<DMLMessage>,
    ) -> Self {
        Self {
            ipc_manager: im,
            cm,
            mtx,
        }
    }

    pub async fn run(self: &Arc<Self>, rurl: Vec<String>) -> Result<()> {
        match self.cm.stream_type.read().await.deref() {
            StreamType::FLV => {
                let s = flv::FLV::new(
                    rurl[0].to_string(),
                    self.cm.clone(),
                    self.ipc_manager.clone(),
                    self.mtx.clone(),
                );
                let s = Arc::new(s);
                let _ = s.run().await;
            }
            StreamType::HLS => {
                let s = hls::HLS::new(
                    rurl[0].to_string(),
                    self.cm.clone(),
                    self.ipc_manager.clone(),
                    self.mtx.clone(),
                );
                let s = Arc::new(s);
                let _ = s.run().await;
            }
            StreamType::DASH => {
                let s = youtube::Youtube::new(
                    rurl[0].to_string(),
                    rurl[1].to_string(),
                    rurl[2].parse().unwrap_or(0),
                    self.cm.clone(),
                    self.ipc_manager.clone(),
                    self.mtx.clone(),
                );
                let s = Arc::new(s);
                s.run().await;
            }
        }
        Ok(())
    }
}
