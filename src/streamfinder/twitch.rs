use crate::dmlerr;
use log::info;
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

    pub async fn get_live_info(client: &reqwest::Client, rid: &str) -> anyhow::Result<(String, String, String, bool)> {
        let payload = format!(
            r#"{{ "query": "query StreamInfo($login: String!) {{ user(login: $login) {{ displayName  login profileImageURL(width: 300)  stream {{ id title  previewImageURL(width: 640, height: 360) game {{ name }} viewersCount }} }} }}", "variables": {{ "login": "{}" }} }}"#,
            rid,
        );
        let resp = client
            .post(TTV_API1)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://m.twitch.tv/")
            .header("Client-Id", "kimne78kx3ncx6brgo4mv6wki5h1ko")
            .body(payload)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        info!("{:?}", &resp);
        let mut is_live = false;
        let owner = resp.pointer("/data/user/displayName").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let avatar = resp.pointer("/data/user/profileImageURL").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let title =
            resp.pointer("/data/user/stream/title").and_then(|x| x.as_str()).map_or("没有直播标题", |x| {
                is_live = true;
                x
            });
        let cover = resp.pointer("/data/user/stream/previewImageURL").and_then(|x| x.as_str()).unwrap_or(avatar);
        Ok((
            owner.to_string(),
            title.to_string(),
            cover.to_string(),
            is_live,
        ))
    }

    pub async fn get_live(&self, room_url: &str) -> anyhow::Result<HashMap<&'static str, String>> {
        let rid = Url::parse(room_url)?.path_segments().and_then(|x| x.last()).ok_or_else(|| dmlerr!())?.to_string();
        let client = reqwest::Client::new();
        let mut ret = HashMap::new();

        let room_info = Self::get_live_info(&client, &rid).await?;
        room_info.3.then(|| 0).ok_or_else(|| dmlerr!())?;
        ret.insert("title", format!("{} - {}", room_info.1, room_info.0));
        let mut param1 = Vec::new();
        let payload = format!(
            r#"{{"query": "query {{ streamPlaybackAccessToken(channelName: \"{}\", params: {{ platform: \"web\", playerBackend:\"mediaplayer\", playerType:\"pulsar\" }}) {{ value, signature }} }}"}}"#,
            &rid,
        );
        let resp = client
            .post(TTV_API1)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://m.twitch.tv/")
            .header("Client-Id", "kimne78kx3ncx6brgo4mv6wki5h1ko")
            .body(payload)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        // println!("{:?}", &resp);
        let sign = resp
            .pointer("/data/streamPlaybackAccessToken/signature")
            .and_then(|x| x.as_str())
            .ok_or_else(|| dmlerr!())?;
        let token =
            resp.pointer("/data/streamPlaybackAccessToken/value").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
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
        let re = Regex::new(r#"BANDWIDTH=([0-9]+)[^\n]+\n(http[^\n]+)"#).unwrap();
        let url = re
            .captures_iter(&resp)
            .map(|x| {
                (
                    x.get(1).map_or("1", |x| x.as_str()),
                    x.get(2).map_or("aaaa", |x| x.as_str()),
                )
            })
            .max_by_key(|x| x.0.parse::<i64>().unwrap_or(0))
            .ok_or_else(|| dmlerr!())?
            .1;
        ret.insert("url", url.to_string());
        Ok(ret)
    }
}
