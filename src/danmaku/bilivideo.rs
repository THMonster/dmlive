use anyhow::Result;
use bytes::BufMut;
use log::info;
use tokio::io::AsyncWriteExt;

pub struct Bilibili {}

impl Bilibili {
    pub fn new() -> Self {
        Bilibili {}
    }

    pub async fn run(
        &self, url: &str, dtx: async_channel::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        for e in elem_dm {
            if e.has_attribute("p") {
                let tmps: Vec<&str> = e.attribute("p").unwrap().split(',').collect();
                dtx.send((
                    format!("{:06x}", tmps[3].parse::<u64>().unwrap_or(16777215)),
                    format!("{},{}", tmps[0], tmps[1]),
                    e.text().unwrap_or("").into(),
                ))
                .await?;
            }
        }
        Ok(())
    }
}
