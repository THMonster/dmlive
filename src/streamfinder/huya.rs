use regex::Regex;
use std::{collections::HashMap, str};

pub struct Huya {}

impl Huya {
    pub fn new() -> Self {
        Huya {}
    }

    pub async fn get_live(&self, room_url: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        // let rid = Url::parse(room_url)?.path_segments().ok_or("rid parse error 1")?.last().ok_or("rid parse error 2")?.to_string();
        let client = reqwest::Client::new();
        let mut ret = HashMap::new();
        let resp =
            client.get(room_url).header("User-Agent", crate::utils::gen_ua()).header("Referer", "https://www.huya.com/").send().await?.text().await?;
        let re = Regex::new(r#"(?m)(?s)hyPlayerConfig.*?stream:(.*?)\s*};"#).unwrap();
        let json_stream = re.captures(&resp).ok_or("regex err 1")?[1].to_string();
        let j: serde_json::Value = serde_json::from_str(json_stream.as_str())?;
        // println!("{:?}", &j);
        ret.insert(
            String::from("title"),
            format!(
                "{} - {}",
                j.pointer("/data/0/gameLiveInfo/roomName").ok_or("json err")?.as_str().ok_or("cannot convert to string")?,
                j.pointer("/data/0/gameLiveInfo/nick").ok_or("json err")?.as_str().ok_or("cannot convert to string")?
            ),
        );
        ret.insert(
            String::from("url"),
            html_escape::decode_html_entities(
                format!(
                    "{}/{}.{}?{}",
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvUrl").ok_or("json err")?.as_str().ok_or("cannot convert to string")?,
                    j.pointer("/data/0/gameStreamInfoList/0/sStreamName").ok_or("json err")?.as_str().ok_or("cannot convert to string")?,
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvUrlSuffix").ok_or("json err")?.as_str().ok_or("cannot convert to string")?,
                    j.pointer("/data/0/gameStreamInfoList/0/sFlvAntiCode").ok_or("json err")?.as_str().ok_or("cannot convert to string")?,
                )
                .as_str(),
            )
            .to_string(),
        );

        Ok(ret)
    }
}
