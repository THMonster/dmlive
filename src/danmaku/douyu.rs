use crate::utils::gen_ua;
use bincode::Options;
use futures::{
    stream::StreamExt,
    SinkExt,
};
use reqwest::Url;
use std::{
    collections::HashMap,
    usize,
};
use tokio::time::sleep;

pub struct Douyu {
    color_tab: HashMap<String, String>,
    heartbeat: Vec<u8>,
}

impl Douyu {
    pub fn new() -> Self {
        let hb = b"\x14\x00\x00\x00\x14\x00\x00\x00\xb1\x02\x00\x00\x74\x79\x70\x65\x40\x3d\x6d\x72\x6b\x6c\x2f\x00"
            .to_vec();
        Douyu {
            color_tab: [
                ("2".to_owned(), "1e87f0".to_owned()),
                ("3".to_owned(), "7ac84b".to_owned()),
                ("4".to_owned(), "ff7f00".to_owned()),
                ("6".to_owned(), "ff69b4".to_owned()),
                ("5".to_owned(), "9b39f4".to_owned()),
                ("1".to_owned(), "ff0000".to_owned()),
            ]
            .iter()
            .cloned()
            .collect::<HashMap<String, String>>(),
            heartbeat: hb,
        }
    }

    async fn get_ws_info(&self, url: &str) -> Result<(String, Vec<Vec<u8>>), Box<dyn std::error::Error>> {
        let mut reg_datas = Vec::new();
        let rid =
            Url::parse(url)?.path_segments().ok_or("rid parse error 1")?.last().ok_or("rid parse error 2")?.to_string();
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

    fn decode_msg(&self, data: &mut Vec<u8>) -> Result<Vec<HashMap<String, String>>, Box<dyn std::error::Error>> {
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

            let msg_type = match j.pointer("/type").ok_or("dm pje 1")?.as_str().ok_or("dm pje 1-2")? {
                "dgb" => "gift",
                "chatmsg" => "danmaku",
                "uenter" => "enter",
                _ => "other",
            };
            let mut d = std::collections::HashMap::new();
            d.insert("msg_type".to_owned(), msg_type.to_owned());
            if msg_type.eq("danmaku") {
                // println!("{:?}", &j);
                d.insert(
                    "name".to_owned(),
                    j.pointer("/nn").ok_or("dm pje 2")?.as_str().ok_or("dm pje 2-2")?.to_owned(),
                );
                d.insert(
                    "content".to_owned(),
                    j.pointer("/txt").ok_or("dm pje 3")?.as_str().ok_or("dm pje 3-2")?.to_owned(),
                );
                let col = match j.pointer("/col").ok_or("dm pje 4") {
                    Ok(it) => {
                        self.color_tab.get(it.as_str().unwrap_or("-1")).unwrap_or(&"ffffff".to_owned()).to_owned()
                    }
                    _ => "ffffff".to_string(),
                };
                d.insert("color".to_owned(), col.to_string());
            }
            ret.push(d);
            data.drain(0..h.0 as usize + 4);
        }
        Ok(ret)
    }
    pub async fn run(
        &self,
        url: &str,
        dtx: async_channel::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
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
        ws_write
            .send(tokio_tungstenite::tungstenite::Message::Binary(
                reg_data[0].to_vec(),
            ))
            .await?;
        ws_write
            .send(tokio_tungstenite::tungstenite::Message::Binary(
                reg_data[1].to_vec(),
            ))
            .await?;
        let hb = self.heartbeat.clone();
        tokio::spawn(async move {
            loop {
                sleep(tokio::time::Duration::from_secs(20)).await;
                let hb1 = hb.clone();
                match ws_write.send(tokio_tungstenite::tungstenite::Message::Binary(hb1)).await {
                    Ok(_) => {}
                    _ => {
                        println!("send heartbeat failed!")
                    }
                };
            }
        });
        while let Some(m) = ws_read.next().await {
            match m {
                Ok(it) => {
                    if let Ok(mut dm) = self.decode_msg(it.into_data().as_mut()) {
                        for d in dm.drain(..) {
                            if d.get("msg_type").unwrap_or(&"other".into()).eq("danmaku") {
                                dtx.send((
                                    d.get("color").unwrap_or(&"ffffff".into()).into(),
                                    d.get("name").unwrap_or(&"unknown".into()).into(),
                                    d.get("content").unwrap_or(&" ".into()).into(),
                                ))
                                .await?;
                            }
                        }
                    }
                }
                Err(e) => {
                    println!("read ws error: {:?}", e)
                }
            }
        }
        println!("ws closed!");
        Ok(())
    }
}
