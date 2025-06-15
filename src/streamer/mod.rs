pub mod flv;
pub mod hls;
pub mod segment;
pub mod youtube;

use crate::{
    config::{ConfigManager, StreamType},
    dmlive::DMLMessage,
    ipcmanager::IPCManager,
};
use std::{collections::HashMap, rc::Rc};

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

    pub async fn run(&self, stream_info: &HashMap<&str, String>) -> anyhow::Result<()> {
        match self.cm.stream_type.get() {
            StreamType::FLV => {
                let s = flv::FLV::new(
                    &stream_info,
                    self.cm.clone(),
                    self.ipc_manager.clone(),
                    self.mtx.clone(),
                );
                s.run().await?;
            }
            StreamType::HLS(_) => {
                let s = hls::HLS::new(
                    &stream_info,
                    self.cm.clone(),
                    self.ipc_manager.clone(),
                    self.mtx.clone(),
                );
                s.run().await?;
            }
            StreamType::DASH => {
                let s = youtube::Youtube::new(
                    &stream_info,
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
