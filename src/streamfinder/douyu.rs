// refer to https://github.com/SeaHOH/ykdl
use chrono::prelude::*;
use log::info;
use std::collections::HashMap;
use uuid::Uuid;

use crate::dmlerr;

const DOUYU_API1: &'static str = "https://www.douyu.com/betard/";
const DOUYU_API2: &'static str = "https://www.douyu.com/swf_api/homeH5Enc?rids=";
const DOUYU_API3: &'static str = "https://www.douyu.com/lapi/live/getH5Play/";

pub struct Douyu {}
impl Douyu {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_live_info(client: &reqwest::Client, rid: &str) -> anyhow::Result<(String, String, String, bool)> {
        let j = client
            .get(format!("{}{}", DOUYU_API1, rid))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://www.douyu.com/")
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        // println!("{:?}", &j);
        let cover = j.pointer("/room/room_pic").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let title = j.pointer("/room/room_name").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let owner = j.pointer("/room/nickname").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let is_living = j.pointer("/room/show_status").and_then(|x| x.as_i64()).ok_or_else(|| dmlerr!())?;
        let is_living2 = j.pointer("/room/videoLoop").and_then(|x| x.as_i64()).ok_or_else(|| dmlerr!())?;
        Ok((
            owner.to_string(),
            title.to_string(),
            cover.to_string(),
            if is_living == 1 && is_living2 == 0 { true } else { false },
        ))
    }

    pub async fn get_live(&self, room_url: &str) -> anyhow::Result<HashMap<&'static str, String>> {
        let mut ret = HashMap::new();
        let rid =
            url::Url::parse(room_url)?.path_segments().and_then(|x| x.last()).ok_or_else(|| dmlerr!())?.to_string();
        let client = reqwest::Client::new();

        let resp = client
            .get(format!("{}{}", DOUYU_API2, &rid))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", format!("https://www.douyu.com/{}", &rid))
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let js_enc = resp.pointer(&format!("/data/room{}", &rid)).and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let crypto_js = include_str!("crypto-js.min.js");
        let did = Uuid::new_v4().as_simple().encode_lower(&mut Uuid::encode_buffer()).to_string();
        let tsec = format!("{}", Local::now().timestamp());

        let rt = rquickjs::Runtime::new()?;
        let ctx = rquickjs::Context::full(&rt)?;
        let enc_data = ctx.with(|ctx| -> rquickjs::Result<String> {
            let _ = ctx.eval::<(), _>(crypto_js)?;
            let _ = ctx.eval::<(), _>(js_enc)?;
            let enc_data = ctx.eval::<String, _>(format!("ub98484234('{rid}','{did}','{tsec}')"))?;
            Ok(enc_data)
        })?;
        info!("{enc_data}");
        let mut param1 = Vec::new();
        enc_data.split('&').for_each(|x| {
            x.split_once('=').map(|x| param1.push(x));
        });
        param1.push(("cdn", ""));
        param1.push(("iar", "0"));
        param1.push(("ive", "0"));
        param1.push(("rate", "0"));
        // println!("{:?}", &param1);

        let resp = client
            .post(format!("{}{}", DOUYU_API3, &rid))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", format!("https://www.douyu.com/{}", &rid))
            .form(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        // println!("{:?}", &resp);
        ret.insert(
            "url",
            format!(
                "{}/{}",
                resp.pointer("/data/rtmp_url").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?,
                resp.pointer("/data/rtmp_live").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?
            ),
        );

        let room_info = Self::get_live_info(&client, &rid).await?;
        ret.insert("title", format!("{} - {}", room_info.1, room_info.0));

        Ok(ret)
    }
}
