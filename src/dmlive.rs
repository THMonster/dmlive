use std::sync::Arc;

use async_channel::Receiver;

use crate::{config::ConfigManager, ipcmanager::IPCManager};

pub enum DMLMessage {
    SetFontSize(usize),
    SetFontAlpha(f64),
    SetShowNick(bool),
    StreamStarted,
}

pub struct DMLive {
    ipc_manager: Arc<crate::ipcmanager::IPCManager>,
    cm: Arc<ConfigManager>,
    mrx: Receiver<DMLMessage>,
}

impl DMLive {
    pub async fn new(cm: Arc<ConfigManager>) -> Self {
        let mut im = IPCManager::new(&cm.room_url, 0);
        im.run().await.unwrap();
        let im = Arc::new(im);
        let (mtx, mrx) = async_channel::unbounded();
        DMLive {
            ipc_manager: im,
            cm,
            mrx,
        }
    }

    pub async fn run(self: &Arc<Self>) {
        todo!()
    }

    pub async fn dispatch(self: &Arc<Self>) {
        loop {
            match self.mrx.recv().await.unwrap() {
                DMLMessage::SetFontSize(_) => todo!(),
                DMLMessage::SetFontAlpha(_) => todo!(),
                DMLMessage::SetShowNick(_) => todo!(),
            }
        }
    }

    pub async fn restart(&self) {
        todo!()
    }

    pub async fn stop(&self) {
        todo!()
    }
}
