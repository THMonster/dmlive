use crate::{dmlerr, utils::gen_ua};
use bincode::Options;
use futures::{stream::StreamExt, SinkExt};
use reqwest::Url;
use std::{collections::HashMap, time::Duration, usize};
use tokio::time::sleep;
use tokio_tungstenite::tungstenite::Message::Binary;

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

    async fn get_ws_info(&self, url: &str) -> anyhow::Result<(String, Vec<Vec<u8>>)> {
        let mut reg_datas = Vec::new();
        let rid =
            Url::parse(url)?.path_segments().ok_or_else(|| dmlerr!())?.last().ok_or_else(|| dmlerr!())?.to_string();
        let mut pl = format!(r#"type@=loginreq/roomid@={}/"#, rid).as_bytes().to_vec();
        let mut data: Vec<u8> = Vec::new();
        let len = pl.len() as u32 + 9;
        data.append(len.to_le_bytes().to_vec().as_mut());
        data.append(len.to_le_bytes().to_vec().as_mut());
        data.append(b"\xb1\x02\x00\x00".to_vec().as_mut());
        data.append(pl.as_mut());
        data.append(b"\x00".to_vec().as_mut());
        reg_datas.push(data);
        let mut pl = format!(r#"type@=joingroup/rid@={}/gid@=1/"#, rid).as_bytes().to_vec();
        let mut data: Vec<u8> = Vec::new();
        let len = pl.len() as u32 + 9;
        data.append(len.to_le_bytes().to_vec().as_mut());
        data.append(len.to_le_bytes().to_vec().as_mut());
        data.append(b"\xb1\x02\x00\x00".to_vec().as_mut());
        data.append(pl.as_mut());
        data.append(b"\x00".to_vec().as_mut());
        reg_datas.push(data);
        Ok(("wss://danmuproxy.douyu.com:8505".to_string(), reg_datas))
    }

    fn decode_msg(&self, data: &mut Vec<u8>) -> anyhow::Result<Vec<HashMap<String, String>>> {
        let mut ret = Vec::new();
        let bc_option = bincode::options().with_little_endian().with_fixint_encoding();
        loop {
            if data.len() <= 13 {
                break;
            }
            let h: (u32, u32) = bc_option.deserialize(&data[0..8])?;
            if data.len() < h.0 as usize {
                break;
            }
            let msg = String::from_utf8_lossy(&data[12..h.0 as usize + 2]);
            let msg = msg.replace("@=", r#"":""#).replace('/', r#"",""#);
            let msg = msg.replace("@A", "@").replace("@S", "/");
            let msg = format!(r#"{{"{}"}}"#, &msg);
            // println!("{}", &msg);
            let j: serde_json::Value = match serde_json::from_str(msg.as_str()) {
                Ok(it) => it,
                _ => {
                    data.drain(0..h.0 as usize + 4);
                    continue;
                }
            };

            let msg_type = match j.pointer("/type").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())? {
                "dgb" => "gift",
                "chatmsg" => "danmaku",
                "uenter" => "enter",
                _ => "other",
            };
            let mut d = std::collections::HashMap::new();
            d.insert("msg_type".to_owned(), msg_type.to_owned());
            if msg_type.eq("danmaku") {
                d.insert(
                    "name".to_owned(),
                    j.pointer("/nn").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?.to_owned(),
                );
                d.insert(
                    "content".to_owned(),
                    j.pointer("/txt").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?.to_owned(),
                );
                let col = j.pointer("/col").map(|it| it.as_str().unwrap_or("-1")).unwrap_or("-1");
                let col = self.color_tab.get(col).unwrap_or(&"ffffff");
                d.insert("color".to_owned(), col.to_string());
            }
            ret.push(d);
            data.drain(0..h.0 as usize + 4);
        }
        Ok(ret)
    }
    pub async fn run(&self, url: &str, dtx: async_channel::Sender<(String, String, String)>) -> anyhow::Result<()> {
        let (ws, reg_data) = self.get_ws_info(url).await?;
        let req = tokio_tungstenite::tungstenite::http::Request::builder()
            .method("GET")
            .header("Host", "danmuproxy.douyu.com")
            .header("Connection", "Upgrade")
            .header("Upgrade", "websocket")
            .header("Sec-WebSocket-Version", "13")
            .header(
                "Sec-WebSocket-Key",
                tokio_tungstenite::tungstenite::handshake::client::generate_key(),
            )
            .header("User-Agent", gen_ua())
            .uri(&ws)
            .body(())?;
        let (ws_stream, _) = tokio_tungstenite::connect_async(req).await?;
        let (mut ws_write, mut ws_read) = ws_stream.split();
        ws_write.send(Binary(reg_data[0].to_vec())).await?;
        ws_write.send(Binary(reg_data[1].to_vec())).await?;
        let hb_task = async {
            while let Ok(_) = ws_write.send(Binary(HEARTBEAT.to_vec())).await {
                sleep(Duration::from_secs(20)).await;
            }
            Err(anyhow::anyhow!("send heartbeat failed!"))
        };
        let recv_task = async {
            while let Some(m) = ws_read.next().await {
                let m = m?;
                let mut dm = self.decode_msg(m.into_data().as_mut())?;
                for mut d in dm.drain(..) {
                    if d.remove("msg_type").unwrap_or("other".into()).eq("danmaku") {
                        dtx.send((
                            d.remove("color").unwrap_or("ffffff".into()),
                            d.remove("name").unwrap_or("unknown".into()),
                            d.remove("content").unwrap_or("".into()),
                        ))
                        .await?;
                    }
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
