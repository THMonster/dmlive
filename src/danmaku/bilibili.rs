use std::{collections::HashMap, io::Read, usize};

use bincode::Options;
use futures::{stream::StreamExt, SinkExt};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;

#[derive(Deserialize, Debug)]
struct BiliDanmakuHeader {
    packet_len: u32,
    header_len: u16,
    ver: u16,
    op: u32,
    seq: u32,
}

pub struct Bilibili {
    api1: String,
    heartbeat: Vec<u8>,
}

impl Bilibili {
    pub fn new() -> Self {
        let hb =
            b"\x00\x00\x00\x1f\x00\x10\x00\x01\x00\x00\x00\x02\x00\x00\x00\x01\x5b\x6f\x62\x6a\x65\x63\x74\x20\x4f\x62\x6a\x65\x63\x74\x5d".to_vec();
        Bilibili {
            api1: "https://api.live.bilibili.com/room/v1/Room/room_init".to_string(),
            heartbeat: hb,
        }
    }

    async fn get_ws_info(
        &self,
        url: &str,
    ) -> Result<(String, Vec<u8>), Box<dyn std::error::Error>> {
        let rid = Url::parse(url)?
            .path_segments()
            .ok_or("rid parse error 1")?
            .last()
            .ok_or("rid parse error 2")?
            .to_string();
        let mut reg_data: Vec<u8> = Vec::new();
        let client = reqwest::Client::new();
        let mut param1 = Vec::new();
        param1.push(("id", rid.as_str()));
        let resp = client
            .get(&self.api1)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", url)
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let rid = resp
            .pointer("/data/room_id")
            .ok_or("gwi pje 1")?
            .as_u64()
            .ok_or("gwi pje 1-2")?;
        let rn = rand::random::<u64>();
        let uid = 1000000 + (rn % 1000000);
        let out_json = json!({"roomid": rid, "uid": uid, "protover": 2});
        let mut out_json = serde_json::to_vec(&out_json)?;
        let len = out_json.len() as u32 + 16;
        reg_data.append(len.to_be_bytes().to_vec().as_mut());
        reg_data.append(b"\x00\x10\x00\x01".to_vec().as_mut());
        reg_data.append(7u32.to_be_bytes().to_vec().as_mut());
        reg_data.append(1u32.to_be_bytes().to_vec().as_mut());
        reg_data.append(&mut out_json);

        Ok((
            "wss://broadcastlv.chat.bilibili.com/sub".to_string(),
            reg_data,
        ))
    }

    fn decode_plain_msg(
        &self,
        header: &BiliDanmakuHeader,
        data: &[u8],
    ) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let mut ret = HashMap::new();
        if header.op == 5 {
            let j: serde_json::Value = serde_json::from_slice(&data)?;
            let msg_type = match j
                .pointer("/cmd")
                .ok_or("dpm pje 1")?
                .as_str()
                .ok_or("dpm pje 1-2")?
            {
                "SEND_GIFT" => "gift",
                "DANMU_MSG" => "danmaku",
                "WELCOME" => "enter",
                "NOTICE_MSG" => "broadcast",
                _ => "other",
            };
            ret.insert("msg_type".to_owned(), msg_type.to_owned());
            if msg_type.eq("danmaku") {
                ret.insert(
                    "name".to_owned(),
                    j.pointer("/info/2/1")
                        .ok_or("dpm pje 2")?
                        .as_str()
                        .ok_or("dpm pje 2-2")?
                        .to_owned(),
                );
                ret.insert(
                    "content".to_owned(),
                    j.pointer("/info/1")
                        .ok_or("dpm pje 3")?
                        .as_str()
                        .ok_or("dpm pje 3-2")?
                        .to_owned(),
                );
                ret.insert(
                    "color".to_owned(),
                    format!(
                        "{:06x}",
                        j.pointer("/info/0/3")
                            .ok_or("dpm pje 4")?
                            .as_u64()
                            .unwrap_or(16777215)
                    ),
                );
            }
        } else {
            ret.insert("msg_type".to_owned(), "other".to_owned());
        }
        Ok(ret)
    }

    fn decode_msg(
        &self,
        data: &mut Vec<u8>,
    ) -> Result<Vec<HashMap<String, String>>, Box<dyn std::error::Error>> {
        let mut ret = Vec::new();
        let bc_option = bincode::options().with_big_endian().with_fixint_encoding();
        loop {
            if data.len() <= 16 {
                break;
            }
            let h: BiliDanmakuHeader = bc_option.deserialize(&data[0..16])?;
            if data.len() < h.packet_len as usize {
                break;
            }
            if h.ver == 1 || h.ver == 0 {
                let dm = self.decode_plain_msg(&h, &data[16..h.packet_len as usize])?;
                ret.push(dm);
            } else if h.ver == 2 {
                let mut decompressed_data: Vec<u8> = Vec::new();
                let mut dp = flate2::read::ZlibDecoder::new(&data[16..h.packet_len as usize]);
                dp.read_to_end(&mut decompressed_data)?;
                ret.append(&mut self.decode_msg(&mut decompressed_data)?);
            }
            data.drain(0..h.packet_len as usize);
        }
        Ok(ret)
    }

    pub async fn run(
        &self,
        url: &str,
        dtx: async_channel::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (ws, reg_data) = self.get_ws_info(url).await?;
        let (ws_stream, _) = connect_async(&ws).await?;
        let (mut ws_write, mut ws_read) = ws_stream.split();
        ws_write
            .send(tokio_tungstenite::tungstenite::Message::Binary(reg_data))
            .await?;
        let hb = self.heartbeat.clone();
        tokio::spawn(async move {
            loop {
                sleep(tokio::time::Duration::from_secs(20)).await;
                let hb1 = hb.clone();
                match ws_write
                    .send(tokio_tungstenite::tungstenite::Message::Binary(hb1))
                    .await
                {
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
                    let mut dm = self.decode_msg(it.into_data().as_mut())?;
                    for d in dm.drain(..) {
                        dtx.send((
                            d.get("color").unwrap_or(&"ffffff".into()).into(),
                            d.get("name").unwrap_or(&"unknown".into()).into(),
                            d.get("content").unwrap_or(&" ".into()).into(),
                        ))
                        .await?;
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
