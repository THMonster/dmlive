use regex::Regex;
use std::{collections::HashMap, str};

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
        ret.insert(
            String::from("url"),
            html_escape::decode_html_entities(
                format!(
                    "{}/{}.{}?{}",
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvUrl").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                    j.pointer("/data/0/gameStreamInfoList/0/sStreamName").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvUrlSuffix").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvAntiCode").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                )
                .as_str(),
            )
            .to_string(),
        );

        Ok(ret)
    }
}
