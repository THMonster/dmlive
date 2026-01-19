use bytes::BufMut;
use log::info;
use std::rc::Rc;
use tokio::io::AsyncWriteExt;

use crate::{danmaku::DMLDanmaku, dmlive::DMLContext};

const BILI_API1: &'static str = "http://api.bilibili.com/x/v1/dm/list.so?oid=";

pub struct Bilibili {
    ctx: Rc<DMLContext>,
}

impl Bilibili {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Bilibili { ctx }
    }

    pub async fn run(&self, dtx: async_channel::Sender<DMLDanmaku>) -> anyhow::Result<()> {
        let client = reqwest::Client::builder()
            .deflate(false)
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let mut resp = client
            .get(format!(
                "{}{}",
                BILI_API1,
                self.ctx.cm.bvideo_info.borrow().current_cid
            ))
            .send()
            .await?;
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            buf.put(chunk);
        }
        info!("{}", self.ctx.cm.bvideo_info.borrow().current_cid);
        let mut dp = async_compression::tokio::write::DeflateDecoder::new(Vec::new());
        dp.write_all(&buf[..]).await?;
        dp.shutdown().await?;
        let dp = dp.into_inner();
        let buf = String::from_utf8_lossy(&dp);
        let doc = roxmltree::Document::parse(&buf)?;
        let elem_dm: Vec<roxmltree::Node> = doc.descendants().filter(|n| n.tag_name().name() == "d").collect();
        for e in elem_dm {
            if e.has_attribute("p") {
                let tmps: Vec<&str> = e.attribute("p").unwrap().split(',').collect();
                let text = e.text().unwrap_or("");
                let time = (tmps[0].parse::<f64>().unwrap() * 1000.0) as i64;
                let position = if tmps[1].eq("4") {
                    2
                } else if tmps[1].eq("5") {
                    8
                } else {
                    0
                };
                let color = format!("{:06x}", tmps[3].parse::<u64>().unwrap_or(16777215));
                let dml_dm = DMLDanmaku {
                    time,
                    text: text.trim().to_string(),
                    nick: "".to_string(),
                    color: color.to_string(),
                    position,
                };
                dtx.send(dml_dm).await?;
                // dtx.send((
                //     format!("{:06x}", tmps[3].parse::<u64>().unwrap_or(16777215)),
                //     format!("{},{}", tmps[0], tmps[1]),
                //     e.text().unwrap_or("").into(),
                // ))
                // .await?;
            }
        }
        dtx.close();
        Ok(())
    }
}
