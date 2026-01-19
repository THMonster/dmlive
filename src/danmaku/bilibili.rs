use bincode::{Decode, Encode};
use bytes::{Buf, BufMut, Bytes};
use chrono::Utc;
use futures::{SinkExt, stream::StreamExt};
use log::info;
use serde_json::json;
use std::{collections::VecDeque, rc::Rc};
use tokio::{
    io::{AsyncReadExt, BufReader},
    sync::mpsc,
    time::{Duration, sleep},
};
use tokio_tungstenite::{connect_async, tungstenite::Message::Binary};
// use wincode::{SchemaRead, SchemaWrite};

use crate::{dmlerr, dmlive::DMLContext};

use super::DMLDanmaku;

const API_BUVID: &'static str = "https://api.bilibili.com/x/frontend/finger/spi";
const API_ROOMINIT: &'static str = "https://api.live.bilibili.com/room/v1/Room/room_init";
const API_DMINFO: &'static str = "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo";
const WS_HEARTBEAT: &'static [u8] = b"\x00\x00\x00\x1f\x00\x10\x00\x01\x00\x00\x00\x02\x00\x00\x00\x01\x5b\x6f\x62\x6a\x65\x63\x74\x20\x4f\x62\x6a\x65\x63\x74\x5d";

#[derive(Encode, Decode, Debug)]
struct BiliDanmakuHeader {
    packet_len: u32,
    header_len: u16,
    ver: u16,
    op: u32,
    seq: u32,
}

pub struct Bilibili {
    ctx: Rc<DMLContext>,
}

impl Bilibili {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Bilibili { ctx }
    }

    async fn get_buvid(&self, client: &reqwest::Client) -> anyhow::Result<(String, String, String)> {
        let resp = client.get(API_BUVID).send().await?.json::<serde_json::Value>().await?;
        let buvid3 = resp.pointer("/data/b_3").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?;
        let buvid4 = resp.pointer("/data/b_4").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?;
        return Ok((
            buvid3.to_string(),
            buvid4.to_string(),
            Utc::now().timestamp().to_string(),
        ));
    }

    async fn get_dm_token(&self, client: &reqwest::Client, cookies: &str) -> anyhow::Result<String> {
        let keys = crate::utils::bili_wbi::get_wbi_keys(&cookies).await?;
        let param1 = vec![("id", self.ctx.cm.room_id.to_string()), ("type", "0".to_string())];
        let query = crate::utils::bili_wbi::encode_wbi(param1, keys);
        info!("{:?}", &query);
        let resp = client
            .get(format!("{}?{}", API_DMINFO, query))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", self.ctx.cm.room_url.as_str())
            .header("Cookie", cookies)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        info!("{:?}", &resp);
        let token = resp.pointer("/data/token").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?;
        Ok(token.to_string())
    }

    async fn get_ws_info(&self) -> anyhow::Result<(String, Bytes)> {
        let mut reg_data = bytes::BytesMut::with_capacity(200);
        let client = reqwest::Client::builder().user_agent(crate::utils::gen_ua()).build()?;
        let (buvid3, buvid4, b_nut) = self.get_buvid(&client).await?;
        let param1 = vec![("id", self.ctx.cm.room_id.as_str())];
        let resp = client
            .get(API_ROOMINIT)
            .header("Referer", self.ctx.cm.room_url.as_str())
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let rid = resp.pointer("/data/room_id").ok_or_else(|| dmlerr!())?.as_u64().ok_or_else(|| dmlerr!())?;
        let cookie = format!("buvid3={}; b_nut={}; buvid4={}", &buvid3, &b_nut, &buvid4);
        let token = self.get_dm_token(&client, &cookie).await?;
        // let rn = rand::random::<u64>();
        // let uid = 1000000 + (rn % 1000000);
        let out_json = json!({"roomid": rid, "uid": 0, "protover": 3, "platform": "web", "type": 2, "buvid": buvid3, "key": token});
        // warn!("out json {:?}", &out_json);
        let out_json = serde_json::to_vec(&out_json)?;
        let len = out_json.len() as u32 + 16;
        reg_data.put_u32(len);
        reg_data.put_slice(b"\x00\x10\x00\x01");
        reg_data.put_u32(7);
        reg_data.put_u32(1);
        reg_data.put_slice(&out_json);
        Ok((
            "wss://broadcastlv.chat.bilibili.com/sub".to_string(),
            reg_data.freeze(),
        ))
    }

    fn decode_plain_msg(&self, header: &BiliDanmakuHeader, data: &[u8]) -> anyhow::Result<DMLDanmaku> {
        if header.op == 5 {
            let j: serde_json::Value = serde_json::from_slice(data)?;
            // warn!("{:?}", &j);
            let msg_type = match j.pointer("/cmd").ok_or_else(|| dmlerr!())?.as_str().unwrap() {
                "SEND_GIFT" => "gift",
                "SUPER_CHAT_MESSAGE" => "superchat",
                "WELCOME" => "enter",
                "NOTICE_MSG" => "broadcast",
                it => {
                    if it.starts_with("DANMU_MSG") {
                        "danmaku"
                    } else {
                        "other"
                    }
                }
            };
            if msg_type.eq("danmaku") {
                let text = j.pointer("/info/1").ok_or_else(|| dmlerr!())?.as_str().unwrap();
                let color = j.pointer("/info/0/3").ok_or_else(|| dmlerr!())?.as_u64().unwrap_or(16777215);
                let nick = j.pointer("/info/2/1").ok_or_else(|| dmlerr!())?.as_str().unwrap();
                let dml_dm = DMLDanmaku {
                    time: 0,
                    text: text.trim().to_string(),
                    nick: nick.to_string(),
                    color: format!("{:06x}", color),
                    position: 0,
                };
                return Ok(dml_dm);
            } else if msg_type.eq("superchat") {
                let text = j.pointer("/data/message").ok_or_else(|| dmlerr!())?.as_str().unwrap();
                let color =
                    j.pointer("/data/background_color_start").ok_or_else(|| dmlerr!())?.as_str().unwrap_or("#FFFFFF");
                let nick = j.pointer("/data/user_info/uname").ok_or_else(|| dmlerr!())?.as_str().unwrap();
                let dml_dm = DMLDanmaku {
                    time: 0,
                    text: format!("[SC]{text}"),
                    nick: nick.to_string(),
                    color: format!("{}", &color[1..]),
                    position: 8,
                };
                return Ok(dml_dm);
            }
        } else {
        }
        Err(anyhow::anyhow!("other msg"))
    }

    async fn decode_msg(
        &self, mut data: Bytes, send_back: &mpsc::Sender<Bytes>,
    ) -> anyhow::Result<VecDeque<DMLDanmaku>> {
        let mut ret = VecDeque::new();
        // let bc_option = bincode::options().with_big_endian().with_fixint_encoding();
        let bc_config = bincode::config::legacy().with_big_endian();

        loop {
            if data.len() <= 16 {
                break;
            }
            let h: BiliDanmakuHeader = bincode::decode_from_slice(&data[0..16], bc_config)?.0;
            // let h: BiliDanmakuHeader = wincode::deserialize(&data[0..16])?;
            if data.len() < h.packet_len as usize {
                break;
            }
            if h.ver == 1 || h.ver == 0 {
                match self.decode_plain_msg(&h, &data[16..h.packet_len as usize]) {
                    Ok(it) => {
                        ret.push_back(it);
                    }
                    Err(e) => {
                        info!("decode_plain_msg: {}", e);
                    }
                };
            } else if h.ver == 2 {
                let mut decoded_data = Vec::new();
                let mut dp = async_compression::tokio::bufread::DeflateDecoder::new(BufReader::new(
                    &data[16..h.packet_len as usize],
                ));
                dp.read_to_end(&mut decoded_data).await?;
                send_back.send(decoded_data.into()).await?;
            } else if h.ver == 3 {
                let mut decoded_data = Vec::new();
                let mut dp = async_compression::tokio::bufread::BrotliDecoder::new(BufReader::new(
                    &data[16..h.packet_len as usize],
                ));
                dp.read_to_end(&mut decoded_data).await?;
                send_back.send(decoded_data.into()).await?;
            }
            data.advance(h.packet_len as usize);
        }
        Ok(ret)
    }

    pub async fn run(&self, dtx: async_channel::Sender<DMLDanmaku>) -> anyhow::Result<()> {
        let (tx, mut rx) = mpsc::channel(10);
        let (ws, reg_data) = self.get_ws_info().await?;
        let (ws_stream, _) = connect_async(&ws).await?;
        let (mut ws_write, mut ws_read) = ws_stream.split();
        ws_write.send(Binary(reg_data.into())).await?;
        let hb_task = async {
            while let Ok(_) = ws_write.send(Binary(WS_HEARTBEAT.into())).await {
                sleep(Duration::from_secs(20)).await;
            }
            Err(anyhow::anyhow!("send heartbeat failed!"))
        };
        let recv_task = async {
            while let Some(m) = ws_read.next().await {
                let m = m?;
                tx.send(m.into_data()).await?;
            }
            Err(anyhow::anyhow!("danmaku ws disconnected!"))
        };

        let (dmq_tx, mut dmq_rx) = mpsc::channel(1000);
        let dm_cnt = std::cell::Cell::new(0u64);
        let decode_task = async {
            while let Some(it) = rx.recv().await {
                let mut dm = self.decode_msg(it, &tx).await?;
                // dm_queue.append(&mut dm);
                for d in dm.drain(..) {
                    dmq_tx.send(d).await?;
                    dm_cnt.set(dm_cnt.get() + 1);
                }
            }
            anyhow::Ok(())
        };
        let balance_task = async {
            while let Some(d) = dmq_rx.recv().await {
                let itvl = {
                    let itvl = 2000u64.saturating_div(dm_cnt.get());
                    dm_cnt.set(dm_cnt.get().saturating_sub(1));
                    itvl
                };
                dtx.send(d).await?;
                if itvl < 50 {
                } else if itvl > 500 {
                    sleep(Duration::from_millis(500)).await;
                } else {
                    sleep(Duration::from_millis(itvl)).await;
                }
            }
            anyhow::Ok(())
        };
        tokio::select! {
            it = hb_task => { it?; },
            it = recv_task => { it?; },
            it = decode_task => { it?; },
            it = balance_task => { it?; },
        }
        Ok(())
    }
}
