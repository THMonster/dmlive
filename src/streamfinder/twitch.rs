use crate::dmlerr;
use regex::Regex;
use std::collections::HashMap;
use url::Url;

const TTV_API1: &'static str = "https://gql.twitch.tv/gql";
const TTV_API2: &'static str = "https://usher.ttvnw.net/api/channel/hls/{channel}.m3u8";

pub struct Twitch {}

impl Twitch {
    pub fn new() -> Self {
        Self {}
    }

    pub fn get_info(&self, html: &str) -> anyhow::Result<String> {
        let re = Regex::new(r#"".+<script type="application/ld\+json">(.+?)</script>.+""#).unwrap();
        let j = re.captures(html).ok_or_else(|| dmlerr!())?.get(1).ok_or_else(|| dmlerr!())?.as_str();
        let j: serde_json::Value = serde_json::from_str(j)?;
        let title = j.pointer("/@graph/0/description").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?;
        Ok(title.to_string())
    }

    pub async fn get_live(&self, room_url: &str) -> anyhow::Result<HashMap<String, String>> {
        let rid = Url::parse(room_url)?
            .path_segments()
            .ok_or_else(|| dmlerr!())?
            .last()
            .ok_or_else(|| dmlerr!())?
            .to_string();
        let client = reqwest::Client::new();
        let mut ret = HashMap::new();
        let resp = client
            .get(format!("https://www.twitch.tv/{}", &rid))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Accept-Language", "en-US")
            .header("Referer", "https://www.twitch.tv/")
            .send()
            .await?
            .text()
            .await?;
        let title = self.get_info(&resp)?;
        ret.insert(String::from("title"), title);
        let mut param1 = Vec::new();
        let qu = format!(
            r#"{{"query": "query {{ streamPlaybackAccessToken(channelName: \"{}\", params: {{ platform: \"web\", playerBackend:\"mediaplayer\", playerType:\"pulsar\" }}) {{ value, signature }} }}"}}"#,
            &rid,
        );
        let resp = client
            .post(TTV_API1)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://m.twitch.tv/")
            // .header("Client-Id", "jzkbprff40iqj646a697cyrvl0zt2m6")
            .header("Client-Id", "kimne78kx3ncx6brgo4mv6wki5h1ko")
            .body(qu)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        // println!("{:?}", &resp);
        let sign = resp
            .pointer("/data/streamPlaybackAccessToken/signature")
            .ok_or_else(|| dmlerr!())?
            .as_str()
            .ok_or_else(|| dmlerr!())?;
        let token = resp
            .pointer("/data/streamPlaybackAccessToken/value")
            .ok_or_else(|| dmlerr!())?
            .as_str()
            .ok_or_else(|| dmlerr!())?;
        param1.clear();
        param1.push(("allow_source", "true"));
        param1.push(("fast_bread", "true"));
        param1.push(("sig", sign));
        param1.push(("token", token));
        let api2 = TTV_API2.replace("{channel}", &rid);
        let resp = client
            .get(api2)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Accept-Language", "en-US")
            .header("Referer", "https://m.twitch.tv/")
            .query(&param1)
            .send()
            .await?
            .text()
            .await?;

        // println!("{}", &resp);
        let re = Regex::new(r#"[\s\S]+?\n(http[^\n]+)"#).unwrap();
        ret.insert(
            "url".to_string(),
            re.captures(&resp).ok_or_else(|| dmlerr!())?.get(1).ok_or_else(|| dmlerr!())?.as_str().to_string(),
        );
        Ok(ret)
    }
}
