pub mod flv;
pub mod hls;
pub mod segment;
pub mod youtube;

use crate::{config::StreamType, dmlive::DMLContext};
use std::rc::Rc;

pub struct Streamer {
    ctx: Rc<DMLContext>,
}

impl Streamer {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Self { ctx }
    }

    pub async fn run(&self) -> anyhow::Result<()> {
        match self.ctx.cm.stream_type.get() {
            StreamType::FLV => {
                let s = flv::FLV::new(self.ctx.clone());
                s.run().await?;
            }
            StreamType::HLS(_) => {
                let s = hls::HLS::new(self.ctx.clone());
                s.run().await?;
            }
            StreamType::DASH => {
                let s = youtube::Youtube::new(self.ctx.clone());
                s.run().await?;
            }
        }
        Ok(())
    }
}
