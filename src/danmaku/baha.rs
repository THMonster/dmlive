use crate::dmlerr;

use super::DMLDanmaku;

pub struct Baha {}

impl Baha {
    pub fn new() -> Self {
        Baha {}
    }

    pub async fn run(&self, sn: String, dtx: async_channel::Sender<DMLDanmaku>) -> anyhow::Result<()> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let url = format!(
            "https://api.gamer.com.tw/anime/v1/danmu.php?geo=TW%2CHK&videoSn={}",
            sn
        );
        let j = client.get(&url).send().await?.json::<serde_json::Value>().await?;
        let j = j.pointer("/data/danmu").ok_or_else(|| dmlerr!())?.as_array().unwrap();
        for d in j {
            let text = d.pointer("/text").ok_or_else(|| dmlerr!())?.as_str().unwrap();
            let time = d.pointer("/time").ok_or_else(|| dmlerr!())?.as_i64().unwrap() * 100;
            let pos = d.pointer("/position").ok_or_else(|| dmlerr!())?.as_i64().unwrap();
            let position = if pos == 0 { 0 } else { 8 };
            let color = d.pointer("/color").ok_or_else(|| dmlerr!())?.as_str().unwrap().strip_prefix("#").unwrap_or("FFFFFF");
            let dml_dm = DMLDanmaku {
                time,
                text: text.to_string(),
                nick: "".to_string(),
                color: color.to_string(),
                position,
            };
            dtx.send(dml_dm).await?;
        }
        dtx.close();
        Ok(())
    }
}
