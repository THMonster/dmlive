pub mod flv;
pub mod hls;
pub mod segment;
pub mod youtube;

use crate::{
    config::{ConfigManager, StreamType},
    dmlive::DMLMessage,
    ipcmanager::IPCManager,
};
use std::rc::Rc;

pub struct Streamer {
    ipc_manager: Rc<IPCManager>,
    cm: Rc<ConfigManager>,
    mtx: async_channel::Sender<DMLMessage>,
}

impl Streamer {
    pub fn new(cm: Rc<ConfigManager>, im: Rc<IPCManager>, mtx: async_channel::Sender<DMLMessage>) -> Self {
        Self {
            ipc_manager: im,
            cm,
            mtx,
        }
    }

    pub async fn run(&self, rurl: &Vec<String>) -> anyhow::Result<()> {
        match self.cm.stream_type.get() {
            StreamType::FLV => {
                let s = flv::FLV::new(
                    rurl[0].to_string(),
                    self.cm.clone(),
                    self.ipc_manager.clone(),
                    self.mtx.clone(),
                );
                s.run().await?;
            }
            StreamType::HLS(_) => {
                let s = hls::HLS::new(
                    rurl[0].to_string(),
                    self.cm.clone(),
                    self.ipc_manager.clone(),
                    self.mtx.clone(),
                );
                s.run().await?;
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
                s.run().await?;
            }
        }
        Ok(())
    }
}
