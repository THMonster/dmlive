pub mod flv;
pub mod hls;
pub mod segment;
pub mod youtube;

use crate::{config::StreamType, dmlive::DMLContext};
use std::{collections::HashMap, rc::Rc};

pub struct Streamer {
    ctx: Rc<DMLContext>,
}

impl Streamer {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Self { ctx }
    }

    pub async fn run(&self, stream_info: &HashMap<&str, String>) -> anyhow::Result<()> {
        match self.ctx.cm.stream_type.get() {
            StreamType::FLV => {
                let s = flv::FLV::new(&stream_info, self.ctx.clone());
                s.run().await?;
            }
            StreamType::HLS(_) => {
                let s = hls::HLS::new(&stream_info, self.ctx.clone());
                s.run().await?;
            }
            StreamType::DASH => {
                let s = youtube::Youtube::new(&stream_info, self.ctx.clone());
                s.run().await?;
            }
        }
        Ok(())
    }
}
