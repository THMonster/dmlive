use regex::Regex;
use std::collections::HashMap;
use url::Url;

pub struct Twitch {
    api1: String,
    api2: String,
}

impl Twitch {
    pub fn new() -> Self {
        Twitch {
            api1: "https://gql.twitch.tv/gql".to_owned(),
            api2: "https://usher.ttvnw.net/api/channel/hls/{channel}.m3u8".to_owned(),
        }
    }

    pub async fn get_live(&self, room_url: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let rid = Url::parse(room_url)?.path_segments().ok_or("rid parse error 1")?.last().ok_or("rid parse error 2")?.to_string();
        let client = reqwest::Client::new();
        let mut ret = HashMap::new();
        let resp = client
            .get(format!("https://m.twitch.tv/{}/profile", &rid))
            .header("User-Agent", crate::utils::gen_ua())
            .header("Accept-Language", "en-US")
            .header("Referer", "https://m.twitch.tv/")
            .send()
            .await?
            .text()
            .await?;
        let re = Regex::new(r#""BroadcastSettings\}\|\{[^"]+":.+?"title":"(.+?)""#).unwrap();
        ret.insert(
            String::from("title"),
            format!(
                "{}",
                re.captures(&resp).ok_or("regex err 1")?[1].to_string()
            ),
        );
        let mut param1 = Vec::new();
        let qu = format!(
            r#"{{"query": "query {{ streamPlaybackAccessToken(channelName: \"{}\", params: {{ platform: \"web\", playerBackend:\"mediaplayer\", playerType:\"pulsar\" }}) {{ value, signature }} }}"}}"#,
            &rid,
        );
        let resp = client
            .post(&self.api1)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://m.twitch.tv/")
            .header("Client-Id", "jzkbprff40iqj646a697cyrvl0zt2m6")
            .body(qu)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        // println!("{:?}", &resp);
        let sign = resp.pointer("/data/streamPlaybackAccessToken/signature").ok_or("gl err 1")?.as_str().ok_or("gl err 1-2")?;
        let token = resp.pointer("/data/streamPlaybackAccessToken/value").ok_or("gl err 2")?.as_str().ok_or("gl err 2-2")?;
        param1.clear();
        param1.push(("allow_source", "true"));
        param1.push(("fast_bread", "true"));
        param1.push(("sig", sign));
        param1.push(("token", token));
        let api2 = self.api2.replace("{channel}", &rid);
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
            String::from("url"),
            format!(
                "{}",
                re.captures(&resp).ok_or("gl err 3")?[1].to_string()
            ),
        );
        Ok(ret)
    }
}
