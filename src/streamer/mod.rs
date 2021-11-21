pub mod flv;
pub mod hls;
pub mod youtube;

use crate::{config::ConfigManager, dmlive::DMLMessage};
use anyhow::*;
use std::sync::Arc;

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

    pub async fn run(self: &Arc<Self>, rurl: &str) -> Result<()> {
        Ok(())
    }
}
