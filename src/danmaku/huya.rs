use futures::{
    stream::StreamExt,
    SinkExt,
};
use regex::Regex;
use reqwest::Url;
use std::collections::HashMap;
use tars_stream::prelude::*;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;

struct HuyaUser {
    _uid: i64,
    _imid: i64,
    name: String,
    _gender: i32,
}

struct HuyaDanmaku {
    color: i32,
}

impl StructFromTars for HuyaUser {
    fn _decode_from(decoder: &mut TarsDecoder) -> Result<Self, DecodeErr> {
        let uid = decoder.read_int64(0, false, -1)?;
        let imid = decoder.read_int64(1, false, -1)?;
        let name = decoder.read_string(2, false, "".to_string())?;
        let gender = decoder.read_int32(3, false, -1)?;
        Ok(HuyaUser {
            _uid: uid,
            _imid: imid,
            name,
            _gender: gender,
        })
    }
}
impl StructFromTars for HuyaDanmaku {
    fn _decode_from(decoder: &mut TarsDecoder) -> Result<Self, DecodeErr> {
        let color = decoder.read_int32(0, false, 16777215)?;
        Ok(HuyaDanmaku { color })
    }
}

pub struct Huya {
    heartbeat: Vec<u8>,
}

impl Huya {
    pub fn new() -> Self {
        let heartbeat =
            b"\x00\x03\x1d\x00\x00\x69\x00\x00\x00\x69\x10\x03\x2c\x3c\x4c\x56\x08\x6f\x6e\x6c\x69\x6e\x65\x75\x69\x66\x0f\x4f\x6e\x55\x73\x65\x72\x48\x65\x61\x72\x74\x42\x65\x61\x74\x7d\x00\x00\x3c\x08\x00\x01\x06\x04\x74\x52\x65\x71\x1d\x00\x00\x2f\x0a\x0a\x0c\x16\x00\x26\x00\x36\x07\x61\x64\x72\x5f\x77\x61\x70\x46\x00\x0b\x12\x03\xae\xf0\x0f\x22\x03\xae\xf0\x0f\x3c\x42\x6d\x52\x02\x60\x5c\x60\x01\x7c\x82\x00\x0b\xb0\x1f\x9c\xac\x0b\x8c\x98\x0c\xa8\x0c".to_vec();
        Huya { heartbeat }
    }

    async fn get_ws_info(&self, url: &str) -> Result<(String, Vec<u8>), Box<dyn std::error::Error>> {
        let url = Url::parse(url)?;
        let rid = url.path_segments().ok_or("gwi err a1")?.last().ok_or("gwi err a12")?;
        let client = reqwest::Client::new();
        let resp = client
            .get(format!("https://www.huya.com/{}", &rid))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://www.huya.com/")
            .send()
            .await?
            .text()
            .await?;
        let re = Regex::new(r"var\s+TT_PROFILE_INFO\s+=\s+(.+\});").unwrap();
        let j: serde_json::Value = serde_json::from_str(&re.captures(&resp).ok_or("gwi err b1")?[1])?;
        let ayyuid = j.pointer("/lp").ok_or("gwi err b2")?.to_string().replace(r#"""#, "");

        let mut t = Vec::new();
        t.push(format!("live:{}", ayyuid));
        t.push(format!("chat:{}", ayyuid));
        let mut oos = TarsEncoder::new();
        oos.write_list(0, &t)?;
        oos.write_string(1, &"".to_owned())?;
        let mut wscmd = TarsEncoder::new();
        wscmd.write_int32(0, 16)?;
        wscmd.write_bytes(1, &oos.to_bytes())?;

        Ok((
            "wss://cdnws.api.huya.com".to_owned(),
            wscmd.to_bytes().to_vec(),
        ))
    }

    fn decode_msg(&self, data: &mut Vec<u8>) -> Result<Vec<HashMap<String, String>>, Box<dyn std::error::Error>> {
        let mut ret = Vec::new();
        // println!("{}", String::from_utf8_lossy(&data));
        let mut ios = TarsDecoder::from(data.to_owned());
        let mut dm = HashMap::new();
        if ios.read_int32(0, false, -1)? == 7 {
            let mut ios = TarsDecoder::from(&ios.read_bytes(1, false, tars_stream::bytes::Bytes::from(""))?);
            if ios.read_int64(1, false, -1)? == 1400 {
                let mut ios = TarsDecoder::from(&ios.read_bytes(2, false, tars_stream::bytes::Bytes::from(""))?);
                let user = ios.read_struct(
                    0,
                    false,
                    HuyaUser {
                        _uid: -1,
                        _imid: -1,
                        name: "".to_owned(),
                        _gender: 1,
                    },
                )?;
                dm.insert("name".to_owned(), user.name);
                dm.insert(
                    "content".to_owned(),
                    ios.read_string(3, false, "".to_owned()).unwrap(),
                );
                let huya_danmaku = ios.read_struct(6, false, HuyaDanmaku { color: 16777215 })?;
                dm.insert(
                    "color".to_owned(),
                    format!(
                        "{:06x}",
                        if huya_danmaku.color == -1 {
                            16777215
                        } else {
                            huya_danmaku.color
                        }
                    ),
                );
                dm.insert("msg_type".to_owned(), "danmaku".to_owned());
            }
        }
        match dm.get("name") {
            Some(it) => {
                if !it.trim().is_empty() {
                    // println!("{:?}", &dm);
                    ret.push(dm);
                }
            }
            _ => {
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "parse failed",
                )));
            }
        };
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
        ws_write.send(tokio_tungstenite::tungstenite::Message::Binary(reg_data)).await?;
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
