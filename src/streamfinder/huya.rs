use base64::{engine::general_purpose, Engine};
use regex::Regex;
use std::{collections::HashMap, str};
use url::form_urlencoded;

use crate::dmlerr;

pub struct Huya {}

impl Huya {
    pub fn new() -> Self {
        Self {}
    }

    pub async fn get_live(&self, room_url: &str) -> anyhow::Result<HashMap<String, String>> {
        let client = reqwest::Client::new();
        let mut ret = HashMap::new();
        let resp = client
            .get(room_url)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://www.huya.com/")
            .send()
            .await?
            .text()
            .await?;
        let re = Regex::new(r#"(?m)(?s)hyPlayerConfig.*?stream:(.*?)\s*};"#).unwrap();
        let re1 = Regex::new(r"var\s+TT_PROFILE_INFO\s+=\s+(.+\});").unwrap();
        let re2 = Regex::new(r"var\s+TT_ROOM_DATA\s+=\s+(.+\});").unwrap();
        let j: serde_json::Value = serde_json::from_str(&re.captures(&resp).ok_or_else(|| dmlerr!())?[1])?;
        let j1: serde_json::Value = serde_json::from_str(&re1.captures(&resp).ok_or_else(|| dmlerr!())?[1])?;
        let j2: serde_json::Value = serde_json::from_str(&re2.captures(&resp).ok_or_else(|| dmlerr!())?[1])?;
        // println!("{:?}", &j);
        ret.insert(
            String::from("title"),
            format!(
                "{} - {}",
                j2.pointer("/introduction").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                j1.pointer("/nick").ok_or_else(|| dmlerr!())?.as_str().unwrap()
            ),
        );
        let flv_anti_code =
            j.pointer("/data/0/gameStreamInfoList/0/sFlvAntiCode").ok_or_else(|| dmlerr!())?.as_str().unwrap();
        let stream_name =
            j.pointer("/data/0/gameStreamInfoList/0/sStreamName").ok_or_else(|| dmlerr!())?.as_str().unwrap();
        let p = Self::gen_params(flv_anti_code, stream_name);
        ret.insert(
            String::from("url"),
            html_escape::decode_html_entities(
                format!(
                    "{}/{}.{}?{}",
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvUrl").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                    stream_name,
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvUrlSuffix").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                    p,
                )
                .as_str(),
            )
            .to_string(),
        );

        Ok(ret)
    }

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

        let uid = Self::gen_n_number(13);
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
                    "{}|{}|{}",
                    seq_id,
                    query.get("ctype").unwrap(),
                    query.get("t").unwrap()
                )
                .as_bytes()
            )
        );
        let ws_secret = format!(
            "{:x}",
            md5::compute(
                format!(
                    "{}_{}_{}_{}_{}",
                    ws_secret_prefix, uid, stream_name, ws_secret_hash, ws_time
                )
                .as_bytes()
            )
        );

        let mut params = vec![
            ("wsSecret", ws_secret),
            ("wsTime", ws_time),
            ("seqid", seq_id),
            ("ctype", query.get("ctype").unwrap().to_string()),
            ("ver", "1".to_string()),
            ("fs", query.get("fs").unwrap().to_string()),
            ("uid", uid),
            ("uuid", Self::gen_n_number(10)),
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

        log::info!("huya params: {:?}", &params);
        form_urlencoded::Serializer::new(String::new()).extend_pairs(params).finish()
    }
}
