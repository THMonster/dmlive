use regex::Regex;
use std::collections::HashMap;
use url::Url;

pub struct Bilibili {
    api1: String,
    api2: String,
    apiv: String,
    apiv_ep: String,
}

impl Bilibili {
    pub fn new() -> Self {
        Bilibili {
            api1: String::from("https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo"),
            api2: String::from("https://api.live.bilibili.com/xlive/web-room/v1/index/getInfoByRoom"),
            apiv: String::from("https://api.bilibili.com/x/player/playurl"),
            apiv_ep: String::from("https://api.bilibili.com/pgc/player/web/playurl"),
        }
    }

    pub async fn get_live(&self, room_url: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let rid = Url::parse(room_url)?.path_segments().ok_or("rid parse error 1")?.last().ok_or("rid parse error 2")?.to_string();
        let client = reqwest::Client::new();
        let mut ret = HashMap::new();
        let mut param1 = Vec::new();
        param1.push(("room_id", rid.as_str()));
        param1.push(("no_playurl", "0"));
        param1.push(("mask", "1"));
        param1.push(("qn", "10000"));
        param1.push(("platform", "web"));
        param1.push(("protocol", "0,1"));
        param1.push(("format", "0,2"));
        param1.push(("codec", "0,1"));

        let resp = client
            .get(&self.api1)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", room_url)
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let j = resp.pointer("/data/playurl_info/playurl/stream/0/format/0/codec/0").ok_or("cannot parse json")?;
        ret.insert(
            String::from("url"),
            format!(
                "{}{}{}",
                j.pointer("/url_info/0/host").ok_or("json err")?.as_str().ok_or("cannot convert to string")?,
                j.pointer("/base_url").ok_or("json err")?.as_str().ok_or("cannot convert to string")?,
                j.pointer("/url_info/0/extra").ok_or("json err")?.as_str().ok_or("cannot convert to string")?
            ),
        );
        param1.clear();
        param1.push(("room_id", rid.as_str()));
        let resp =
            client.get(&self.api2).header("User-Agent", crate::utils::gen_ua()).query(&param1).send().await?.json::<serde_json::Value>().await?;
        ret.insert(
            String::from("title"),
            format!(
                "{} - {}",
                resp.pointer("/data/room_info/title").ok_or("json err")?.as_str().ok_or("cannot convert to string")?,
                resp.pointer("/data/anchor_info/base_info/uname").ok_or("json err")?.as_str().ok_or("cannot convert to string")?
            ),
        );
        Ok(ret)
    }
    pub async fn get_page_info_ep(&self, video_url: &str) -> Result<(String, String, String, String, String), Box<dyn std::error::Error>> {
        let client = reqwest::Client::new();
        let resp = client.get(video_url).header("User-Agent", crate::utils::gen_ua()).header("Referer", video_url).send().await?.text().await?;
        let re = Regex::new(r"__INITIAL_STATE__=(\{.+?\});").unwrap();
        let j: serde_json::Value = serde_json::from_str(re.captures(&resp).ok_or("gpie regex err 1")?[1].to_string().as_ref())?;
        // println!("{:?}", &j);
        let title = match j.pointer("/h1Title") {
            Some(it) => it.as_str().ok_or("cannot convert to string")?.to_string(),
            _ => {
                let re = Regex::new(r"<title>(.+?)_番剧_bilibili_哔哩哔哩<").unwrap();
                re.captures(&resp).ok_or("gpie regex err 2")?[1].to_string()
            }
        };
        let cid = j.pointer("/epInfo/cid").ok_or("json err")?.as_u64().ok_or("cannot convert to u64")?.to_string();
        let bvid = j.pointer("/epInfo/bvid").ok_or("json err")?.as_str().ok_or("cannot convert to string")?.to_string();
        let artist = j.pointer("/mediaInfo/upInfo/name").ok_or("json err")?.as_str().ok_or("cannot convert to string")?.to_string();
        let season_type = match j.pointer("/mediaInfo/season_type") {
            Some(it) => it.as_i64().ok_or("cannot convert to string")?.to_string(),
            _ => j.pointer("/mediaInfo/ssType").ok_or("json err")?.as_i64().ok_or("cannot convert to string")?.to_string(),
        };

        Ok((bvid, cid, title, artist, season_type))
    }

    pub async fn get_page_info(&self, video_url: &str) -> Result<(String, String, String, String), Box<dyn std::error::Error>> {
        let re = Regex::new(r"\?p=(\d+)").unwrap();
        let page_index = match re.captures(video_url) {
            Some(it) => it[1].to_string(),
            _ => "1".to_string(),
        };
        let client = reqwest::Client::new();
        let resp = client.get(video_url).header("User-Agent", crate::utils::gen_ua()).header("Referer", video_url).send().await?;
        let resp = resp.text().await?;
        let re = Regex::new(r"__INITIAL_STATE__=(\{.+?\});").unwrap();
        let j: serde_json::Value = serde_json::from_str(re.captures(&resp).ok_or("gpi regex err 1")?[1].to_string().as_ref())?;
        let bvid = j.pointer("/videoData/bvid").ok_or("json err")?.as_str().ok_or("cannot convert to string")?.to_string();
        let mut title = j.pointer("/videoData/title").ok_or("json err")?.as_str().ok_or("cannot convert to string")?.to_string();
        let artist = j.pointer("/videoData/owner/name").ok_or("json err")?.as_str().ok_or("cannot convert to string")?.to_string();
        let mut cid = String::new();
        let j = j.pointer("/videoData/pages").ok_or("json err")?.as_array().ok_or("cannot convert to array")?;
        for p in j {
            let i = p.pointer("/page").ok_or("json err")?.as_u64().ok_or("cannot convert to u64")?;
            let subtitle = match p.pointer("/part").ok_or("json err")?.as_str() {
                Some(it) => it,
                _ => "",
            };
            if page_index.eq(format!("{}", i).as_str()) {
                cid.push_str(p.pointer("/cid").ok_or("json err")?.as_u64().ok_or("cannot convert to u64")?.to_string().as_str());
                if i > 1 {
                    let t = title.clone();
                    title.clear();
                    title.push_str(format!("{} - {} - {}", &t, &i, &subtitle).as_str());
                }
            }
        }
        Ok((bvid, cid, title, artist))
    }

    pub async fn get_video(&self, video_url: &str, cookie: &str) -> Result<Vec<String>, Box<dyn std::error::Error>> {
        let mut ret: Vec<String> = Vec::new();
        if video_url.contains("bilibili.com/bangumi") {
            let (bvid, cid, title, _artist, _season_type) = self.get_page_info_ep(video_url).await?;
            ret.push(title);
            let mut param1 = Vec::new();
            param1.push(("cid", cid.as_str()));
            param1.push(("bvid", bvid.as_str()));
            param1.push(("qn", "120"));
            param1.push(("otype", "json"));
            param1.push(("fourk", "1"));
            param1.push(("fnver", "0"));
            param1.push(("fnval", "16"));
            let client = reqwest::Client::new();
            let resp = client
                .get(&self.apiv_ep)
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/83.0.4103.106 Safari/537.36",
                )
                .header("Referer", video_url)
                .header("Cookie", cookie)
                .query(&param1)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            // println!("{:?}", &resp);
            let j = resp.pointer("/result").ok_or("get_video pje 1")?;
            if j.pointer("/dash").is_some() {
                let dash_id = j.pointer("/dash/video/0/id").ok_or("get_video pje 2")?.as_i64().ok_or("get_video ce 1")?;
                if j.pointer("/dash/video").ok_or("get_video pje 3")?.as_array().ok_or("cannot convert to vec")?.len() > 1
                    && dash_id == j.pointer("/dash/video/0/id").ok_or("get_video pje 4")?.as_i64().ok_or("")?
                {
                    if j.pointer("/dash/video/0/codecid").ok_or("get_video pje n")?.as_i64().ok_or("")? == 12 {
                        ret.push(j.pointer("/dash/video/1/base_url").ok_or("get_video pje 5")?.as_str().ok_or("")?.to_string());
                        ret.push(j.pointer("/dash/audio/0/base_url").ok_or("get_video pje 6")?.as_str().ok_or("")?.to_string());
                        ret.push(j.pointer("/dash/video/0/base_url").ok_or("get_video pje 7")?.as_str().ok_or("")?.to_string());
                    } else {
                        ret.push(j.pointer("/dash/video/0/base_url").ok_or("get_video pje 8")?.as_str().ok_or("")?.to_string());
                        ret.push(j.pointer("/dash/audio/0/base_url").ok_or("get_video pje 9")?.as_str().ok_or("")?.to_string());
                        ret.push(j.pointer("/dash/video/1/base_url").ok_or("get_video pje 10")?.as_str().ok_or("")?.to_string());
                    }
                } else {
                    ret.push(j.pointer("/dash/video/0/base_url").ok_or("get_video pje 11")?.as_str().ok_or("")?.to_string());
                    ret.push(j.pointer("/dash/audio/0/base_url").ok_or("get_video pje 12")?.as_str().ok_or("")?.to_string());
                }
            } else {
                let videos = j.pointer("/durl").ok_or("get_video pje 13")?.as_array().ok_or("")?;
                for v in videos {
                    ret.push(v.pointer("url").ok_or("get_video pje 14")?.as_str().ok_or("")?.to_string());
                }
            }
        } else {
            let (bvid, cid, title, artist) = self.get_page_info(video_url).await?;
            println!("{} {} {} {}", &bvid, &cid, &title, &artist);
            ret.push(title);
            let mut param1 = Vec::new();
            param1.push(("cid", cid.as_str()));
            param1.push(("bvid", bvid.as_str()));
            param1.push(("qn", "120"));
            param1.push(("otype", "json"));
            param1.push(("fourk", "1"));
            param1.push(("fnver", "0"));
            param1.push(("fnval", "16"));
            let client = reqwest::Client::new();
            let resp = client
                .get(&self.apiv)
                .header(
                    "User-Agent",
                    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/83.0.4103.106 Safari/537.36",
                )
                .header("Referer", video_url)
                .header("Cookie", cookie)
                .query(&param1)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            let j = resp.pointer("/data").ok_or("get_video pje 15")?;
            if j.pointer("/dash").is_some() {
                let dash_id = j.pointer("/dash/video/0/id").ok_or("get_video pje 16")?.as_i64().ok_or("")?;
                if j.pointer("/dash/video").ok_or("get_video pje 17")?.as_array().ok_or("cannot convert to vec")?.len() > 1
                    && dash_id == j.pointer("/dash/video/0/id").ok_or("get_video pje 18")?.as_i64().ok_or("")?
                {
                    if j.pointer("/dash/video/0/codecid").ok_or("get_video pje 19")?.as_i64().ok_or("")? == 12 {
                        ret.push(j.pointer("/dash/video/1/base_url").ok_or("get_video pje 20")?.as_str().ok_or("")?.to_string());
                        ret.push(j.pointer("/dash/audio/0/base_url").ok_or("get_video pje 21")?.as_str().ok_or("")?.to_string());
                        ret.push(j.pointer("/dash/video/0/base_url").ok_or("get_video pje 22")?.as_str().ok_or("")?.to_string());
                    } else {
                        ret.push(j.pointer("/dash/video/0/base_url").ok_or("get_video pje 23")?.as_str().ok_or("")?.to_string());
                        ret.push(j.pointer("/dash/audio/0/base_url").ok_or("get_video pje 24")?.as_str().ok_or("")?.to_string());
                        ret.push(j.pointer("/dash/video/1/base_url").ok_or("get_video pje 25")?.as_str().ok_or("")?.to_string());
                    }
                } else {
                    ret.push(j.pointer("/dash/video/0/base_url").ok_or("get_video pje 26")?.as_str().ok_or("")?.to_string());
                    ret.push(j.pointer("/dash/audio/0/base_url").ok_or("get_video pje 27")?.as_str().ok_or("")?.to_string());
                }
            } else {
                let videos = j.pointer("/durl").ok_or("get_video pje 28")?.as_array().ok_or("")?;
                for v in videos {
                    ret.push(v.pointer("url").ok_or("get_video pje 29")?.as_str().ok_or("")?.to_string());
                }
            }
        }
        Ok(ret)
    }
}
