use crate::{dmlerr, utils::gen_ua};
use bytes::{Buf, BufMut, Bytes};
use futures::{stream::StreamExt, SinkExt};
use reqwest::Url;
use std::{collections::HashMap, time::Duration};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, Message::Binary};

use super::DMLDanmaku;

const HEARTBEAT: &'static [u8] =
    b"\x14\x00\x00\x00\x14\x00\x00\x00\xb1\x02\x00\x00\x74\x79\x70\x65\x40\x3d\x6d\x72\x6b\x6c\x2f\x00";

pub struct Douyu {
    color_tab: HashMap<&'static str, &'static str>,
}

impl Douyu {
    pub fn new() -> Self {
        let mut ct = HashMap::new();
        ct.insert("1", "ff0000");
        ct.insert("2", "1e87f0");
        ct.insert("3", "7ac84b");
        ct.insert("4", "ff7f00");
        ct.insert("5", "9b39f4");
        ct.insert("6", "ff69b4");
        Douyu { color_tab: ct }
    }

    async fn get_ws_info(&self, url: &str) -> anyhow::Result<(String, Vec<Bytes>)> {
        let mut reg_datas = Vec::new();
        let rid =
            Url::parse(url)?.path_segments().ok_or_else(|| dmlerr!())?.last().ok_or_else(|| dmlerr!())?.to_string();
        let pl = format!(r#"type@=loginreq/roomid@={}/"#, rid);
        let mut data = bytes::BytesMut::with_capacity(100);
        let len = pl.len() as u32 + 9;
        data.put_u32_le(len);
        data.put_u32_le(len);
        data.put_slice(b"\xb1\x02\x00\x00");
        data.put_slice(pl.as_bytes());
        data.put_slice(b"\x00");
        reg_datas.push(data.freeze());
        let pl = format!(r#"type@=joingroup/rid@={}/gid@=1/"#, rid);
        let mut data = bytes::BytesMut::with_capacity(100);
        let len = pl.len() as u32 + 9;
        data.put_u32_le(len);
        data.put_u32_le(len);
        data.put_slice(b"\xb1\x02\x00\x00");
        data.put_slice(pl.as_bytes());
        data.put_slice(b"\x00");
        reg_datas.push(data.freeze());
        Ok(("wss://danmuproxy.douyu.com:8505".to_string(), reg_datas))
    }

    fn decode_msg(&self, mut data: Bytes) -> anyhow::Result<Vec<DMLDanmaku>> {
        let mut ret = Vec::new();
        // let bc_config = bincode::config::legacy();
        loop {
            if data.len() <= 12 {
                break;
            }
            let msg_len = data.get_u32_le() as usize;
            if data.len() < msg_len {
                break;
            }
            data.advance(8);
            let b = data.split_to(msg_len - 8 - 2);
            let msg = String::from_utf8_lossy(&b);
            let msg = msg.replace("@=", r#"":""#).replace('/', r#"",""#);
            let msg = msg.replace("@A", "@").replace("@S", "/");
            let msg = format!(r#"{{"{}"}}"#, &msg);
            // println!("{}", &msg);
            let j: serde_json::Value = match serde_json::from_str(msg.as_str()) {
                Ok(it) => it,
                _ => {
                    data.advance(2);
                    continue;
                }
            };

            let msg_type = match j.pointer("/type").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())? {
                "dgb" => "gift",
                "chatmsg" => "danmaku",
                "uenter" => "enter",
                _ => "other",
            };
            if msg_type.eq("danmaku") {
                let text = j.pointer("/txt").ok_or_else(|| dmlerr!())?.as_str().unwrap();
                let color = j.pointer("/col").map(|it| it.as_str().unwrap_or("-1")).unwrap_or("-1");
                let nick = j.pointer("/nn").ok_or_else(|| dmlerr!())?.as_str().unwrap();
                let dml_dm = DMLDanmaku {
                    time: 0,
                    text: text.to_string(),
                    nick: nick.to_string(),
                    color: self.color_tab.get(color).unwrap_or(&"ffffff").to_string(),
                    position: 0,
                };
                ret.push(dml_dm);
            }
            data.advance(2);
        }
        Ok(ret)
    }
    pub async fn run(&self, url: &str, dtx: async_channel::Sender<DMLDanmaku>) -> anyhow::Result<()> {
        let (ws, reg_data) = self.get_ws_info(url).await?;
        let mut req = ws.into_client_request().unwrap();
        req.headers_mut().insert("User-Agent", gen_ua().parse().unwrap());
        let (ws_stream, _) = tokio_tungstenite::connect_async(req).await?;
        let (mut ws_write, mut ws_read) = ws_stream.split();
        ws_write.send(Binary(reg_data[0].clone())).await?;
        ws_write.send(Binary(reg_data[1].clone())).await?;
        let hb_task = async {
            while let Ok(_) = ws_write.send(Binary(HEARTBEAT.into())).await {
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
