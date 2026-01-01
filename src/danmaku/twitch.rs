use crate::dmlerr;
use bytes::Bytes;
use futures::{SinkExt, stream::StreamExt};
use regex::Regex;
use reqwest::Url;
use tokio::time::{Duration, sleep};
use tokio_tungstenite::{connect_async, tungstenite::Message};

use super::DMLDanmaku;

const HEARTBEAT: &'static str = "PING";

pub struct Twitch {}

impl Twitch {
    pub fn new() -> Self {
        Self {}
    }

    async fn get_ws_info(&self, url: &str) -> anyhow::Result<(String, Vec<String>)> {
        let rid =
            Url::parse(url)?.path_segments().ok_or_else(|| dmlerr!())?.last().ok_or_else(|| dmlerr!())?.to_string();
        let mut reg_datas: Vec<String> = Vec::new();

        reg_datas.push("CAP REQ :twitch.tv/tags twitch.tv/commands twitch.tv/membership".to_owned());
        reg_datas.push("PASS SCHMOOPIIE".to_owned());
        let rn = rand::random::<u64>();
        let nick = format!("justinfan{}", 10000 + (rn % 80000));
        reg_datas.push(format!("NICK {}", &nick));
        reg_datas.push(format!("USER {0} 8 * :{0}", &nick));
        reg_datas.push(format!("JOIN #{}", &rid));
        // println!("{:?}", &reg_datas);
        Ok(("wss://irc-ws.chat.twitch.tv".to_string(), reg_datas))
    }

    fn decode_msg(&self, data: Bytes) -> anyhow::Result<Vec<DMLDanmaku>> {
        let mut ret = Vec::new();
        let msg = String::from_utf8_lossy(&data);
        for m in msg.split('\n') {
            let re = Regex::new(r#"display-name=([^;]+);"#).unwrap();
            let nick = match re.captures(m) {
                Some(it) => it[1].to_string(),
                _ => continue,
            };
            let re = Regex::new(r#"PRIVMSG [^:]+:(.+)"#).unwrap();
            let text = match re.captures(m) {
                Some(it) => it[1].trim().to_string(),
                _ => continue,
            };
            let re = Regex::new(r#"color=#([a-zA-Z0-9]{6});"#).unwrap();
            let color = match re.captures(m) {
                Some(it) => it[1].to_string(),
                None => "ffffff".to_owned(),
            };
            let dml_dm = DMLDanmaku {
                time: 0,
                text,
                nick,
                color,
                position: 0,
            };
            ret.push(dml_dm);
        }
        Ok(ret)
    }

    pub async fn run(&self, url: &str, dtx: async_channel::Sender<DMLDanmaku>) -> anyhow::Result<()> {
        let (ws, mut reg_datas) = self.get_ws_info(url).await?;
        let (ws_stream, _) = connect_async(&ws).await?;
        let (mut ws_write, mut ws_read) = ws_stream.split();
        for reg_data in reg_datas.drain(..) {
            ws_write.send(Message::text(reg_data)).await?;
        }
        let hb_task = async {
            while let Ok(_) = ws_write.send(Message::text(HEARTBEAT)).await {
                sleep(Duration::from_secs(20)).await;
            }
            Err(anyhow::anyhow!("send heartbeat failed!"))
        };
        let recv_task = async {
            while let Some(m) = ws_read.next().await {
                let m = m?;
                let mut dm = self.decode_msg(m.into_data())?;
                for d in dm.drain(..) {
                    dtx.send(d).await?;
                }
            }
            anyhow::Ok(())
        };
        tokio::select! {
            it = hb_task => { it?; },
            it = recv_task => { it?; },
        }
        Ok(())
    }
}
