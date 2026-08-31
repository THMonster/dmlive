use bytes::BufMut;
use log::info;
use tokio::io::AsyncWriteExt;

use super::DMLDanmaku;

pub struct Bilibili {}

impl Bilibili {
    pub fn new() -> Self {
        Bilibili {}
    }

    pub async fn run(&self, url: &str, dtx: async_channel::Sender<DMLDanmaku>) -> anyhow::Result<()> {
        for dml_dm in self.fetch(url).await? {
            dtx.send(dml_dm).await?;
        }
        dtx.close();
        Ok(())
    }

    pub async fn fetch(&self, url: &str) -> anyhow::Result<Vec<DMLDanmaku>> {
        let client = reqwest::Client::builder()
            .deflate(false)
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let mut resp = client.get(url).send().await?;
        let mut buf: Vec<u8> = Vec::new();
        while let Some(chunk) = resp.chunk().await? {
            buf.put(chunk);
        }
        info!("{}", &url);
        let mut dp = async_compression::tokio::write::DeflateDecoder::new(Vec::new());
        dp.write_all(&buf[..]).await?;
        dp.shutdown().await?;
        let dp = dp.into_inner();
        let buf = String::from_utf8_lossy(&dp);
        let doc = roxmltree::Document::parse(&buf)?;
        let elem_dm: Vec<roxmltree::Node> = doc.descendants().filter(|n| n.tag_name().name() == "d").collect();
        let mut danmaku = Vec::with_capacity(elem_dm.len());
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
                danmaku.push(dml_dm);
            }
        }
        Ok(danmaku)
    }
}
