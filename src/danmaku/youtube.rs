use base64::{
    engine::general_purpose,
    Engine,
};
use chrono::prelude::*;
use log::*;
use regex::Regex;
use reqwest::Client;
use serde_json::{
    json,
    Value,
};
use std::{
    collections::HashMap,
    sync::Arc,
};
use tokio::time::sleep;

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
            key: String::from_utf8_lossy(
                general_purpose::STANDARD.decode(b"eW91dHViZWkvdjEvbGl2ZV9jaGF0L2dldF9saXZlX2NoYXQ/a2V5PUFJemFTeUFPX0ZKMlNscVU4UTRTVEVITEdDaWx3X1k5XzExcWNXOA==")
                    .unwrap()
                    .as_ref(),
            )
            .to_string(),
            ua: "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/84.0.4147.135 Safari/537.36".to_owned(),
        }
    }

    async fn get_room_info(&self, url: &str) -> Result<(String, String), Box<dyn std::error::Error>> {
        let cid: String;
        let vid: String;
        let client = reqwest::Client::new();
        if url.contains("youtube.com/channel/") {
            cid = url::Url::parse(url)?
                .path_segments()
                .ok_or("rid parse error 1")?
                .last()
                .ok_or("rid parse error 2")?
                .to_string();
            let ch_url = format!("https://www.youtube.com/channel/{}/videos", &cid);
            let resp = client
                .get(&ch_url)
                .header("User-Agent", crate::utils::gen_ua())
                .header("Accept-Language", "en-US")
                .header("Referer", "https://www.youtube.com/")
                .send()
                .await?
                .text()
                .await?;
            let re = fancy_regex::Regex::new(r#""gridVideoRenderer"((.(?!"gridVideoRenderer"))(?!"style":"UPCOMING"))+"label":"(LIVE|LIVE NOW|PREMIERING NOW)"([\s\S](?!"style":"UPCOMING"))+?("gridVideoRenderer"|</script>)"#).unwrap();
            let t = re.captures(&resp)?.ok_or("gri err 1")?.get(0).ok_or("gri err 1-2")?.as_str();
            let re = Regex::new(r#""gridVideoRenderer".+?"videoId":"(.+?)""#).unwrap();
            vid = re.captures(t).ok_or("gri err 2")?[1].to_string();
        } else {
            let re = Regex::new(r"youtube.com/watch\?v=([^/?]+)").unwrap();
            vid = re.captures(url).ok_or("gri err 3")?[1].to_string();
            let resp = client
                .get(format!("https://www.youtube.com/embed/{}", &vid))
                .header("User-Agent", crate::utils::gen_ua())
                .header("Accept-Language", "en-US")
                .header("Referer", "https://www.youtube.com/")
                .send()
                .await?
                .text()
                .await?;
            let re = Regex::new(r#"\\"channelId\\":\\"(.{24})\\""#).unwrap();
            cid = re.captures(&resp).ok_or("gri err 4")?[1].to_string();
        }
        // println!("{} {}", &vid, &cid);
        Ok((vid, cid))
    }

    fn decode_msg(&self, j: &Value) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let mut d = std::collections::HashMap::new();
        let renderer = j.pointer("/addChatItemAction/item/liveChatTextMessageRenderer").ok_or("dm err 1")?;
        d.insert(
            "name".to_owned(),
            renderer.pointer("/authorName/simpleText").ok_or("dm err 2")?.as_str().ok_or("dm err 2-2")?.to_string(),
        );
        let runs = renderer.pointer("/message/runs").ok_or("dm err 3")?.as_array().ok_or("dm err 3-2")?;
        let mut msg = "".to_owned();
        for r in runs {
            match r.pointer("/emoji") {
                Some(it) => {
                    msg.push_str(it.pointer("/shortcuts/0").ok_or("dm err 4")?.as_str().ok_or("dm err 4-2")?);
                }
                None => {
                    msg.push_str(r.pointer("/text").ok_or("dm err 5")?.as_str().ok_or("dm err 5-2")?);
                }
            }
        }
        d.insert("content".to_owned(), msg);
        d.insert("msg_type".to_owned(), "danmaku".to_owned());
        Ok(d)
    }

    async fn get_single_chat(
        &self,
        ctn: &mut String,
        client: Arc<Client>,
    ) -> Result<Vec<HashMap<String, String>>, Box<dyn std::error::Error>> {
        let mut ret = Vec::new();
        let body = json!({
            "context": {
                "client": {
                    "visitorData": "",
                    "userAgent": &self.ua,
                    "clientName": "WEB",
                    "clientVersion": format!("2.{}.01.00", (Utc::now() - chrono::Duration::days(2)).format("%Y%m%d")),
                },
            },
            "continuation": &ctn,
        });
        let body = serde_json::to_vec(&body)?;
        // println!("{}", String::from_utf8_lossy(&body));

        // let client = reqwest::Client::new();
        let resp = client
            .post(format!("https://www.youtube.com/{}", &self.key))
            .header("Connection", "keep-alive")
            .header("User-Agent", &self.ua)
            .body(body)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        ctn.clear();
        // println!("{:#?}", &resp);
        let con = resp.pointer("/continuationContents/liveChatContinuation/continuations/0").ok_or("gsc err 1")?;

        // println!("{:#?}", &con);
        let metadata = match con.pointer("/invalidationContinuationData") {
            Some(it) => it,
            _ => match con.pointer("/timedContinuationData") {
                Some(it) => it,
                None => match con.pointer("/reloadContinuationData") {
                    Some(it) => it,
                    None => con.pointer("/liveChatReplayContinuationData").ok_or("gsc err 2")?,
                },
            },
        };
        ctn.push_str(metadata.pointer("/continuation").ok_or("gsc err 3")?.as_str().ok_or("gsc err 3-2")?);
        let actions = resp
            .pointer("/continuationContents/liveChatContinuation/actions")
            .ok_or("gsc err 4")?
            .as_array()
            .ok_or("gsc err 4-2")?;
        for action in actions {
            if let Ok(it) = self.decode_msg(action) {
                ret.push(it);
            }
        }

        Ok(ret)
    }

    pub async fn run(
        &self,
        url: &str,
        dtx: async_channel::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (vid, cid) = self.get_room_info(url).await?;
        let mut ctn = get_param(&vid, &cid);
        let http_client = Arc::new(reqwest::Client::new());

        loop {
            if ctn.trim().is_empty() {
                info!("ctn not found, regenerate...");
                ctn.push_str(&get_param(&vid, &cid));
            }
            let interval: u64;
            match self.get_single_chat(&mut ctn, http_client.clone()).await {
                Ok(mut dm) => {
                    if !dm.is_empty() {
                        interval = 2000 / dm.len() as u64;
                        for d in dm.drain(..) {
                            if d.get("msg_type").unwrap_or(&"other".into()).eq("danmaku") {
                                dtx.send((
                                    d.get("color").unwrap_or(&"ffffff".into()).into(),
                                    d.get("name").unwrap_or(&"unknown".into()).into(),
                                    d.get("content").unwrap_or(&" ".into()).into(),
                                ))
                                .await?;
                                sleep(tokio::time::Duration::from_millis(interval)).await;
                            }
                        }
                    } else {
                        sleep(tokio::time::Duration::from_secs(2)).await;
                    }
                }
                Err(e) => {
                    info!("{}", e);
                    sleep(tokio::time::Duration::from_secs(2)).await;
                }
            }
        }
    }
}
