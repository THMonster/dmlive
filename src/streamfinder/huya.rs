use base64::{Engine, engine::general_purpose};
use regex::Regex;
use std::{collections::HashMap, rc::Rc, str};
use url::form_urlencoded;

use crate::{dmlerr, dmlive::DMLContext};

fn gen_n_number(l: u8) -> String {
    let mut ret = String::new();
    let rn = rand::random::<u32>();
    let n1 = 49 + (rn % 9);
    ret.push(char::from_u32(n1).unwrap());
    for _ in 0..(l - 1) {
        let rn = rand::random::<u32>();
        let n1 = 48 + (rn % 10);
        ret.push(char::from_u32(n1).unwrap())
    }
    ret
}

fn gen_params(anti_code: &str, stream_name: &str) -> String {
    let mut query: HashMap<String, String> = form_urlencoded::parse(anti_code.as_bytes()).into_owned().collect();

    let uid = gen_n_number(13);
    query.insert("t".to_string(), "102".to_string());
    query.insert("ctype".to_string(), "tars_mp".to_string());

    let ws_time = format!("{:x}", (chrono::Utc::now().timestamp() + 21600));
    let seq_id = format!(
        "{}",
        (chrono::Utc::now().timestamp_millis() + uid.parse::<i64>().unwrap())
    );

    let fm = String::from_utf8(general_purpose::STANDARD.decode(query.get("fm").unwrap()).unwrap()).unwrap();
    let ws_secret_prefix = fm.split("_").next().unwrap();
    let ws_secret_hash = format!(
        "{:x}",
        md5::compute(
            format!(
                "{seq_id}|{}|{}",
                query.get("ctype").unwrap(),
                query.get("t").unwrap()
            )
            .as_bytes()
        )
    );
    let ws_secret =
        md5::compute(format!("{ws_secret_prefix}_{uid}_{stream_name}_{ws_secret_hash}_{ws_time}",).as_bytes());
    let ws_secret = format!("{ws_secret:x}",);

    let mut params = vec![
        ("wsSecret", ws_secret),
        ("wsTime", ws_time),
        ("seqid", seq_id),
        ("ctype", query.get("ctype").unwrap().to_string()),
        ("ver", "1".to_string()),
        ("fs", query.get("fs").unwrap().to_string()),
        ("uid", uid),
        ("uuid", gen_n_number(10)),
        ("t", query.get("t").unwrap().to_string()),
        ("sv", "2401231033".to_string()),
        // ("sv", "2110211124".to_string()),
    ];

    if let Some(sphdcdn) = query.get("sphdcdn") {
        params.push(("sphdcdn", sphdcdn.to_string()));
    }
    if let Some(sphd_dc) = query.get("sphdDC") {
        params.push(("sphdDC", sphd_dc.to_string()));
    }
    if let Some(sphd) = query.get("sphd") {
        params.push(("sphd", sphd.to_string()));
    }
    if let Some(exsphd) = query.get("exsphd") {
        params.push(("exsphd", exsphd.to_string()));
    }

    log::info!("huya params: {params:?}");
    form_urlencoded::Serializer::new(String::new()).extend_pairs(params).finish()
}

pub async fn get_live_info(
    client: &reqwest::Client, url: &str,
) -> anyhow::Result<(String, String, String, bool, String)> {
    let resp = client
        .get(url)
        .header("User-Agent", crate::utils::gen_ua())
        .header("Referer", "https://www.huya.com/")
        .send()
        .await?
        .text()
        .await?;
    let re = Regex::new(r#"(?m)(?s)hyPlayerConfig.*?stream:(.*?)\s*};"#).unwrap();
    let re1 = Regex::new(r"var\s+TT_PROFILE_INFO\s+=\s+(.+\});").unwrap();
    let re2 = Regex::new(r"var\s+TT_ROOM_DATA\s+=\s+(.+\});").unwrap();
    let j: serde_json::Value =
        serde_json::from_str(re.captures(&resp).and_then(|x| x.get(1)).ok_or_else(|| dmlerr!())?.as_str())?;
    let j1: serde_json::Value =
        serde_json::from_str(re1.captures(&resp).and_then(|x| x.get(1)).ok_or_else(|| dmlerr!())?.as_str())?;
    let j2: serde_json::Value =
        serde_json::from_str(re2.captures(&resp).and_then(|x| x.get(1)).ok_or_else(|| dmlerr!())?.as_str())?;
    let title = j2.pointer("/introduction").and_then(|x| x.as_str()).unwrap_or("没有直播标题");
    let nick = j1.pointer("/nick").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
    let avatar = j1.pointer("/avatar").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
    let cover = j2.pointer("/screenshot").and_then(|x| x.as_str()).unwrap_or(avatar);
    let is_living = j2.pointer("/isOn").and_then(|x| x.as_bool()).ok_or_else(|| dmlerr!())?;
    let cover = if cover.starts_with("//") {
        format!("https:{cover}")
    } else {
        cover.to_string()
    };

    let vurl = if is_living {
        let flv_anti_code =
            j.pointer("/data/0/gameStreamInfoList/0/sFlvAntiCode").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let stream_name =
            j.pointer("/data/0/gameStreamInfoList/0/sStreamName").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let p = gen_params(flv_anti_code, stream_name);
        let vurl = format!(
            "{}/{stream_name}.{}?{p}",
            j.pointer("/data/0/gameStreamInfoList/0/sFlvUrl").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?,
            j.pointer("/data/0/gameStreamInfoList/0/sFlvUrlSuffix")
                .and_then(|x| x.as_str())
                .ok_or_else(|| dmlerr!())?,
        );
        html_escape::decode_html_entities(vurl.as_str()).to_string()
    } else {
        "".to_string()
    };

    Ok((nick.to_string(), title.to_string(), cover, is_living, vurl))
}

pub struct Huya {
    ctx: Rc<DMLContext>,
}

impl Huya {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Self { ctx }
    }

    pub async fn get_live(&self) -> anyhow::Result<()> {
        let client = reqwest::Client::new();
        let room_info = get_live_info(&client, &self.ctx.cm.room_url).await?;
        room_info.3.then(|| 0).ok_or_else(|| dmlerr!())?;
        let mut si = self.ctx.cm.stream_info.borrow_mut();
        si.insert("owner_name", room_info.0);
        si.insert("title", room_info.1);
        si.insert("cover", room_info.2);
        si.insert("url", room_info.4);
        Ok(())
    }
}
