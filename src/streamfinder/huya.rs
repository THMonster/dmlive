use regex::Regex;
use std::{
    collections::HashMap,
    str,
};

pub struct Huya {}

impl Huya {
    pub fn new() -> Self {
        Huya {}
    }

    pub async fn get_live(&self, room_url: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        // let rid = Url::parse(room_url)?.path_segments().ok_or("rid parse error 1")?.last().ok_or("rid parse error 2")?.to_string();
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
        let j: serde_json::Value = serde_json::from_str(&re.captures(&resp).ok_or("gl err a1")?[1])?;
        let j1: serde_json::Value = serde_json::from_str(&re1.captures(&resp).ok_or("gl err a2")?[1])?;
        let j2: serde_json::Value = serde_json::from_str(&re2.captures(&resp).ok_or("gl err a3")?[1])?;
        // println!("{:?}", &j);
        ret.insert(
            String::from("title"),
            format!(
                "{} - {}",
                j2.pointer("/introduction").ok_or("gl err b1")?.as_str().unwrap(),
                j1.pointer("/nick").ok_or("gl err b2")?.as_str().unwrap()
            ),
        );
        ret.insert(
            String::from("url"),
            html_escape::decode_html_entities(
                format!(
                    "{}/{}.{}?{}",
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvUrl").ok_or("gl err b3")?.as_str().unwrap(),
                    j.pointer("/data/0/gameStreamInfoList/0/sStreamName").ok_or("gl err b4")?.as_str().unwrap(),
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvUrlSuffix").ok_or("gl err b5")?.as_str().unwrap(),
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvAntiCode").ok_or("gl err b6")?.as_str().unwrap(),
                )
                .as_str(),
            )
            .to_string(),
        );

        Ok(ret)
    }
}
