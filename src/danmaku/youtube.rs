use crate::{dmlerr, utils};
use base64::{engine::general_purpose, Engine};
use chrono::prelude::*;
use log::*;
use regex::Regex;
use reqwest::Client;
use serde_json::{json, Value};
use std::collections::HashMap;
use tokio::time::{sleep, Duration};

const YTB_KEY: &'static [u8] =
    b"eW91dHViZWkvdjEvbGl2ZV9jaGF0L2dldF9saXZlX2NoYXQ/a2V5PUFJemFTeUFPX0ZKMlNscVU4UTRTVEVITEdDaWx3X1k5XzExcWNXOA==";

fn get_param(vid: &str, cid: &str) -> String {
    let ts = Utc::now().timestamp() as u64 * 1000000;
    let chattype = 1;
    let b1 = crate::utils::nm(1, 0);
    let b2 = crate::utils::nm(2, 0);
    let b3 = crate::utils::nm(3, 0);
    let b4 = crate::utils::nm(4, 0);
    let b7 = crate::utils::rs(7, b"");
    let b8 = crate::utils::nm(8, 0);
    let b9 = crate::utils::rs(9, b"");
    let timestamp2 = crate::utils::nm(10, ts);
    let b11 = crate::utils::nm(11, 3);
    let b15 = crate::utils::nm(15, 0);

    let s1_3: Vec<u8> = crate::utils::rs(1, vid.as_bytes());
    let s1_5 = [crate::utils::rs(1, cid.as_bytes()), crate::utils::rs(2, vid.as_bytes())].concat();
    let s1 = [crate::utils::rs(3, s1_3.as_ref()), crate::utils::rs(5, s1_5.as_ref())].concat();
    let s3 = crate::utils::rs(48687757, crate::utils::rs(1, vid.as_bytes()).as_ref());
    let header = [
        crate::utils::rs(1, s1.as_ref()),
        crate::utils::rs(3, s3.as_ref()),
        crate::utils::nm(4, 1),
    ]
    .concat();

    let header = crate::utils::rs(3, general_purpose::STANDARD.encode(header).as_bytes());
    let timestamp1 = crate::utils::nm(5, ts);
    let s6 = crate::utils::nm(6, 0);
    let s7 = crate::utils::nm(7, 0);
    let s8 = crate::utils::nm(8, 1);
    let mut tmp = Vec::new();
    tmp.extend(b1);
    tmp.extend(b2);
    tmp.extend(b3);
    tmp.extend(b4);
    tmp.extend(b7);
    tmp.extend(b8);
    tmp.extend(b9);
    tmp.extend(timestamp2);
    tmp.extend(b11);
    tmp.extend(b15);
    let body = crate::utils::rs(9, tmp.as_ref());
    let timestamp3 = crate::utils::nm(10, ts);
    let timestamp4 = crate::utils::nm(11, ts);
    let s13 = crate::utils::nm(13, chattype);
    let chattype = crate::utils::rs(16, crate::utils::nm(1, chattype).as_ref());
    let s17 = crate::utils::nm(17, 0);
    let str19 = crate::utils::rs(19, crate::utils::nm(1, 0).as_ref());
    let timestamp5 = crate::utils::nm(20, ts);
    let entity = [
        header, timestamp1, s6, s7, s8, body, timestamp3, timestamp4, s13, chattype, s17, str19, timestamp5,
    ]
    .concat();
    let continuation = crate::utils::rs(119693434, entity.as_ref());
    url::form_urlencoded::byte_serialize(general_purpose::URL_SAFE.encode(continuation).as_bytes()).collect()
}

pub struct Youtube {
    key: String,
    ua: String,
}

impl Youtube {
    pub fn new() -> Self {
        Youtube {
            key: String::from_utf8_lossy(general_purpose::STANDARD.decode(YTB_KEY).unwrap().as_ref()).to_string(),
            ua: utils::gen_ua(),
        }
    }

    async fn get_room_info(&self, url: &str, client: &Client) -> anyhow::Result<(String, String)> {
        let url = url::Url::parse(url)?;
        let room_url = if url.as_str().contains("youtube.com/channel/") {
            let cid = url.path_segments().ok_or_else(|| dmlerr!())?.last().ok_or_else(|| dmlerr!())?;
            format!("https://www.youtube.com/channel/{}/live", &cid)
        } else {
            for q in url.query_pairs() {
                if q.0.eq("v") {}
            }
            let vid = url.query_pairs().find(|q| q.0.eq("v")).unwrap().1;
            format!("https://www.youtube.com/watch?v={}", &vid)
        };
        let resp = client
            .get(&room_url)
            .header("Connection", "keep-alive")
            .header("Accept-Language", "en-US")
            .header("Referer", "https://www.youtube.com/")
            .send()
            .await?
            .text()
            .await?;
        let re = Regex::new(r"ytInitialPlayerResponse\s*=\s*(\{.+?\});.*?</script>").unwrap();
        let j: serde_json::Value = serde_json::from_str(&re.captures(&resp).ok_or_else(|| dmlerr!())?[1])?;
        let vid = j.pointer("/videoDetails/videoId").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        let cid = j.pointer("/videoDetails/channelId").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        // println!("{} {}", &vid, &cid);
        Ok((vid, cid))
    }

    fn decode_msg(&self, j: &Value) -> anyhow::Result<HashMap<String, String>> {
        let mut d = std::collections::HashMap::new();
        let renderer = j.pointer("/addChatItemAction/item/liveChatTextMessageRenderer").ok_or_else(|| dmlerr!())?;
        d.insert(
            "name".to_owned(),
            renderer
                .pointer("/authorName/simpleText")
                .ok_or_else(|| dmlerr!())?
                .as_str()
                .ok_or_else(|| dmlerr!())?
                .to_string(),
        );
        let runs = renderer.pointer("/message/runs").ok_or_else(|| dmlerr!())?.as_array().ok_or_else(|| dmlerr!())?;
        let mut msg = "".to_owned();
        for r in runs {
            match r.pointer("/emoji") {
                Some(it) => {
                    msg.push_str(
                        it.pointer("/shortcuts/0").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
                    );
                }
                None => {
                    msg.push_str(r.pointer("/text").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?);
                }
            }
        }
        d.insert("content".to_owned(), msg);
        d.insert("msg_type".to_owned(), "danmaku".to_owned());
        Ok(d)
    }

    async fn get_single_chat(&self, ctn: &mut String, client: &Client) -> anyhow::Result<Vec<HashMap<String, String>>> {
        let mut ret = Vec::new();
        let body = json!({
            "context": {
                "client": {
                    "visitorData": "",
                    "userAgent": self.ua,
                    "clientName": "WEB",
                    "clientVersion": format!("2.{}.01.00", (Utc::now() - chrono::Duration::days(2)).format("%Y%m%d")),
                },
            },
            "continuation": &ctn,
        });
        let body = serde_json::to_vec(&body)?;
        // println!("{}", String::from_utf8_lossy(&body));

        let resp = client
            .post(format!("https://www.youtube.com/{}", &self.key))
            .header("Connection", "keep-alive")
            .body(body)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        ctn.clear();
        // println!("{:#?}", &resp);
        let con =
            resp.pointer("/continuationContents/liveChatContinuation/continuations/0").ok_or_else(|| dmlerr!())?;

        // println!("{:#?}", &con);
        let metadata = match con.pointer("/invalidationContinuationData") {
            Some(it) => it,
            _ => match con.pointer("/timedContinuationData") {
                Some(it) => it,
                None => match con.pointer("/reloadContinuationData") {
                    Some(it) => it,
                    None => con.pointer("/liveChatReplayContinuationData").ok_or_else(|| dmlerr!())?,
                },
            },
        };
        ctn.push_str(metadata.pointer("/continuation").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?);
        let actions = resp
            .pointer("/continuationContents/liveChatContinuation/actions")
            .ok_or_else(|| dmlerr!())?
            .as_array()
            .ok_or_else(|| dmlerr!())?;
        for action in actions {
            if let Ok(it) = self.decode_msg(action) {
                ret.push(it);
            }
        }

        Ok(ret)
    }

    pub async fn run(&self, url: &str, dtx: async_channel::Sender<(String, String, String)>) -> anyhow::Result<()> {
        let client =
            reqwest::Client::builder().user_agent(self.ua.clone()).connect_timeout(Duration::from_secs(10)).build()?;
        let (vid, cid) = self.get_room_info(url, &client).await?;
        let mut ctn = get_param(&vid, &cid);

        let mut interval = tokio::time::interval(Duration::from_millis(2000));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            if ctn.trim().is_empty() {
                info!("ctn not found, regenerate...");
                ctn.push_str(&get_param(&vid, &cid));
            }
            let itvl: u64;
            match self.get_single_chat(&mut ctn, &client).await {
                Ok(mut dm) => {
                    itvl = 2000usize.saturating_div(if dm.len() == 0 { 1 } else { dm.len() }) as u64;
                    for d in dm.drain(..) {
                        if d.get("msg_type").unwrap_or(&"other".into()).eq("danmaku") {
                            dtx.send((
                                d.get("color").unwrap_or(&"ffffff".into()).into(),
                                d.get("name").unwrap_or(&"unknown".into()).into(),
                                d.get("content").unwrap_or(&" ".into()).into(),
                            ))
                            .await?;
                            if itvl < 50 {
                            } else if itvl > 500 {
                                sleep(Duration::from_millis(500)).await;
                            } else {
                                sleep(Duration::from_millis(itvl)).await;
                            }
                        }
                    }
                }
                Err(e) => {
                    info!("get single chat error: {}", e);
                }
            }
        }
    }
}
