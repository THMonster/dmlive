use std::collections::{HashMap, VecDeque};

use bincode::Options;
use futures::{stream::StreamExt, SinkExt};
use reqwest::Url;
use serde::Deserialize;
use serde_json::json;
use tokio::{
    io::AsyncWriteExt,
    sync::mpsc,
    time::{sleep, Duration},
};
use tokio_tungstenite::{connect_async, tungstenite::Message::Binary};

use crate::dmlerr;

const API_BUVID: &'static str = "https://api.bilibili.com/x/frontend/finger/spi";
const API_ROOMINIT: &'static str = "https://api.live.bilibili.com/room/v1/Room/room_init";
const API_DMINFO: &'static str = "https://api.live.bilibili.com/xlive/web-room/v1/index/getDanmuInfo";
const WS_HEARTBEAT: &'static [u8] = b"\x00\x00\x00\x1f\x00\x10\x00\x01\x00\x00\x00\x02\x00\x00\x00\x01\x5b\x6f\x62\x6a\x65\x63\x74\x20\x4f\x62\x6a\x65\x63\x74\x5d";

#[allow(dead_code)]
#[derive(Deserialize)]
struct BiliDanmakuHeader {
    packet_len: u32,
    header_len: u16,
    ver: u16,
    op: u32,
    seq: u32,
}

pub struct Bilibili {}

impl Bilibili {
    pub fn new() -> Self {
        Bilibili {}
    }

    async fn get_buvid(&self, client: &reqwest::Client) -> anyhow::Result<String> {
        let resp = client.get(API_BUVID).send().await?.json::<serde_json::Value>().await?;
        let buvid3 = resp.pointer("/data/b_3").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?;
        return Ok(buvid3.to_string());
    }

    async fn get_dm_token(&self, client: &reqwest::Client, url: &str, rid: &str) -> anyhow::Result<String> {
        let param1 = vec![("id", rid), ("type", "0")];
        let resp = client
            .get(API_DMINFO)
            .header("Referer", url)
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let token = resp.pointer("/data/token").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?;
        Ok(token.to_string())
    }

    async fn get_ws_info(&self, url: &str) -> anyhow::Result<(String, Vec<u8>)> {
        let rid =
            Url::parse(url)?.path_segments().ok_or_else(|| dmlerr!())?.last().ok_or_else(|| dmlerr!())?.to_string();
        let mut reg_data: Vec<u8> = Vec::new();
        let client = reqwest::Client::builder().user_agent(crate::utils::gen_ua()).build()?;
        let param1 = vec![("id", rid.as_str())];
        let resp = client
            .get(API_ROOMINIT)
            .header("Referer", url)
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let rid = resp.pointer("/data/room_id").ok_or_else(|| dmlerr!())?.as_u64().ok_or_else(|| dmlerr!())?;
        let buvid = self.get_buvid(&client).await?;
        let token = self.get_dm_token(&client, url, rid.to_string().as_str()).await?;
        // let rn = rand::random::<u64>();
        // let uid = 1000000 + (rn % 1000000);
        let out_json =
            json!({"roomid": rid, "uid": 0, "protover": 3, "platform": "web", "type": 2, "buvid": buvid, "key": token});
        // warn!("out json {:?}", &out_json);
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

    fn decode_plain_msg(&self, header: &BiliDanmakuHeader, data: &[u8]) -> anyhow::Result<HashMap<String, String>> {
        let mut ret = HashMap::new();
        if header.op == 5 {
            let j: serde_json::Value = serde_json::from_slice(data)?;
            // warn!("{:?}", &j);
            let msg_type = match j.pointer("/cmd").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())? {
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
            ret.insert("msg_type".to_owned(), msg_type.to_owned());
            if msg_type.eq("danmaku") {
                let mut f1 = || {
                    ret.insert(
                        "name".to_owned(),
                        j.pointer("/info/2/1")?.as_str()?.to_owned(),
                    );
                    ret.insert(
                        "content".to_owned(),
                        j.pointer("/info/1")?.as_str()?.to_owned(),
                    );
                    ret.insert(
                        "color".to_owned(),
                        format!(
                            "{:06x}",
                            j.pointer("/info/0/3")?.as_u64().unwrap_or(16777215)
                        ),
                    );
                    Some(())
                };
                f1().ok_or_else(|| anyhow::anyhow!("danmaku decode failed"))?;
            } else if msg_type.eq("superchat") {
                let mut f1 = || {
                    ret.insert(
                        "name".to_owned(),
                        j.pointer("/data/user_info/uname")?.as_str()?.to_owned(),
                    );
                    ret.insert(
                        "content".to_owned(),
                        format!("[SC]{}", j.pointer("/data/message")?.as_str()?),
                    );
                    let mut c = j.pointer("/data/background_color_start")?.as_str()?.to_lowercase();
                    c.remove(0);
                    ret.insert("color".to_owned(), c);
                    *ret.get_mut("msg_type").unwrap() = "danmaku".into();
                    Some(())
                };
                f1().ok_or_else(|| anyhow::anyhow!("superchat decode failed"))?;
            }
        } else {
            ret.insert("msg_type".to_owned(), "other".to_owned());
        }
        Ok(ret)
    }

    async fn decode_msg(
        &self, data: &mut Vec<u8>, send_back: &mpsc::Sender<Vec<u8>>,
    ) -> anyhow::Result<VecDeque<HashMap<String, String>>> {
        let mut ret = VecDeque::new();
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
                ret.push_back(dm);
            } else if h.ver == 2 {
                let mut dp = async_compression::tokio::write::DeflateDecoder::new(Vec::new());
                dp.write_all(&data[16..h.packet_len as usize]).await?;
                dp.shutdown().await?;
                let dp = dp.into_inner();
                send_back.send(dp).await?;
            } else if h.ver == 3 {
                let mut dp = async_compression::tokio::write::BrotliDecoder::new(Vec::new());
                dp.write_all(&data[16..h.packet_len as usize]).await?;
                dp.shutdown().await?;
                let dp = dp.into_inner();
                send_back.send(dp).await?;
            }
            data.drain(0..h.packet_len as usize);
        }
        Ok(ret)
    }

    pub async fn run(&self, url: &str, dtx: async_channel::Sender<(String, String, String)>) -> anyhow::Result<()> {
        let (tx, mut rx) = mpsc::channel::<Vec<u8>>(10);
        let (ws, reg_data) = self.get_ws_info(url).await?;
        let (ws_stream, _) = connect_async(&ws).await?;
        let (mut ws_write, mut ws_read) = ws_stream.split();
        ws_write.send(Binary(reg_data)).await?;
        let hb_task = async {
            while let Ok(_) = ws_write.send(Binary(WS_HEARTBEAT.to_vec())).await {
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
            while let Some(mut it) = rx.recv().await {
                let mut dm = self.decode_msg(&mut it, &tx).await?;
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
                if d.get("msg_type").unwrap_or(&"other".into()).eq("danmaku") {
                    dtx.send((
                        d.get("color").unwrap_or(&"ffffff".into()).into(),
                        d.get("name").unwrap_or(&"unknown".into()).into(),
                        d.get("content").unwrap_or(&" ".into()).into(),
                    ))
                    .await?;
                }
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
