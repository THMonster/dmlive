use crate::config::ConfigManager;
use anyhow::anyhow;
use anyhow::Result;
use regex::Regex;
use std::borrow::Cow;
use std::{
    collections::HashMap,
    sync::Arc,
};
use url::Url;

pub struct Bilibili {
    api1: String,
    api2: String,
    apiv: String,
    apiv_ep: String,
    cm: Arc<ConfigManager>,
}

impl Bilibili {
    pub fn new(cm: Arc<ConfigManager>) -> Self {
        Bilibili {
            api1: String::from("https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo"),
            api2: String::from("https://api.live.bilibili.com/xlive/web-room/v1/index/getInfoByRoom"),
            apiv: String::from("https://api.bilibili.com/x/player/playurl"),
            apiv_ep: String::from("https://api.bilibili.com/pgc/player/web/playurl"),
            cm,
        }
    }

    pub async fn get_live(&self, room_url: &str) -> Result<HashMap<String, String>> {
        let rid = Url::parse(room_url)?
            .path_segments()
            .ok_or(anyhow!("rid parse error 1"))?
            .last()
            .ok_or(anyhow!("rid parse error 2"))?
            .to_string();
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;

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

        let cookie = if self.cm.plive { self.cm.bcookie.as_str() } else { "" };
        let resp = client
            .get(&self.api1)
            .header("Referer", room_url)
            .header("Cookie", cookie)
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        let j =
            resp.pointer("/data/playurl_info/playurl/stream/0/format/0/codec/0").ok_or(anyhow!("cannot parse json"))?;
        ret.insert(
            String::from("url"),
            format!(
                "{}{}{}",
                j.pointer("/url_info/0/host")
                    .ok_or(anyhow!("json err"))?
                    .as_str()
                    .ok_or(anyhow!("cannot convert to string"))?,
                j.pointer("/base_url")
                    .ok_or(anyhow!("json err"))?
                    .as_str()
                    .ok_or(anyhow!("cannot convert to string"))?,
                j.pointer("/url_info/0/extra")
                    .ok_or(anyhow!("json err"))?
                    .as_str()
                    .ok_or(anyhow!("cannot convert to string"))?
            ),
        );
        param1.clear();
        param1.push(("room_id", rid.as_str()));
        let resp = client.get(&self.api2).query(&param1).send().await?.json::<serde_json::Value>().await?;
        ret.insert(
            String::from("title"),
            format!(
                "{} - {}",
                resp.pointer("/data/room_info/title")
                    .ok_or(anyhow!("json err"))?
                    .as_str()
                    .ok_or(anyhow!("cannot convert to string"))?,
                resp.pointer("/data/anchor_info/base_info/uname")
                    .ok_or(anyhow!("json err"))?
                    .as_str()
                    .ok_or(anyhow!("cannot convert to string"))?
            ),
        );
        Ok(ret)
    }
    pub async fn get_page_info_ep(&self, video_url: &str) -> Result<(String, String, String, String, String)> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(video_url).header("Referer", video_url).send().await?.text().await?;
        let re = Regex::new(r"__INITIAL_STATE__=(\{.+?\});").unwrap();
        let j: serde_json::Value =
            serde_json::from_str(re.captures(&resp).ok_or(anyhow!("gpie regex err 1"))?[1].to_string().as_ref())?;
        // println!("{:?}", &j);
        let title = match j.pointer("/h1Title") {
            Some(it) => it.as_str().ok_or(anyhow!("cannot convert to string"))?.to_string(),
            _ => {
                let re = Regex::new(r"<title>(.+?)_番剧_bilibili_哔哩哔哩<").unwrap();
                re.captures(&resp).ok_or(anyhow!("gpie regex err 2"))?[1].to_string()
            }
        };
        let mut cid = j.pointer("/epInfo/cid").ok_or(anyhow!("json err 1"))?.as_i64().unwrap().to_string();
        let mut bvid = match j.pointer("/epInfo/bvid") {
            Some(it) => it.as_str().unwrap().to_string(),
            None => "".into(),
        };
        let artist = j.pointer("/mediaInfo/upInfo/name").ok_or(anyhow!("json err3"))?.as_str().unwrap().to_string();
        let season_type = match j.pointer("/mediaInfo/season_type") {
            Some(it) => it.as_i64().ok_or(anyhow!("cannot convert to string"))?.to_string(),
            _ => j.pointer("/mediaInfo/ssType").ok_or(anyhow!("json err"))?.as_i64().unwrap().to_string(),
        };
        let eplist = j.pointer("/epList").ok_or(anyhow!("json err 4"))?.as_array().unwrap();
        self.cm.bvideo_info.write().await.plist.clear();
        for (i, p) in eplist.iter().enumerate() {
            if i == 0 && bvid.is_empty() {
                bvid.push_str(p.pointer("/bvid").ok_or(anyhow!("json err 5"))?.as_str().unwrap());
                cid = p.pointer("/cid").ok_or(anyhow!("json err 6"))?.as_i64().unwrap().to_string();
            }
            if p.pointer("/bvid").ok_or(anyhow!("json err"))?.as_str().unwrap().eq(&bvid) {
                self.cm.bvideo_info.write().await.current_page = i + 1;
            }
            self.cm.bvideo_info.write().await.plist.push(format!(
                "ep{}",
                p.pointer("/id").ok_or(anyhow!("json err"))?.as_u64().unwrap()
            ));
        }
        Ok((bvid, cid, title, artist, season_type))
    }

    pub async fn get_page_info(&self, video_url: &str) -> Result<(String, String, String, String)> {
        let re = Regex::new(r"\?p=(\d+)").unwrap();
        let page_index = match re.captures(video_url) {
            Some(it) => it[1].to_string(),
            _ => "1".to_string(),
        };
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(video_url).header("Referer", video_url).send().await?;
        let resp = resp.text().await?;
        let re = Regex::new(r"__INITIAL_STATE__=(\{.+?\});").unwrap();
        let j: serde_json::Value =
            serde_json::from_str(re.captures(&resp).ok_or(anyhow!("gpi regex err 1"))?[1].to_string().as_ref())?;
        let bvid = j.pointer("/videoData/bvid").ok_or(anyhow!("json err"))?.as_str().unwrap().to_string();
        let mut title = j.pointer("/videoData/title").ok_or(anyhow!("json err"))?.as_str().unwrap().to_string();
        let artist = j.pointer("/videoData/owner/name").ok_or(anyhow!("json err"))?.as_str().unwrap().to_string();
        let mut cid = String::new();
        let j = j.pointer("/videoData/pages").ok_or(anyhow!("json err"))?.as_array().unwrap();
        self.cm.bvideo_info.write().await.plist = vec!["".into(); j.len()];
        for p in j {
            let i = p.pointer("/page").ok_or(anyhow!("json err"))?.as_u64().unwrap();
            let subtitle = match p.pointer("/part").ok_or(anyhow!("json err"))?.as_str() {
                Some(it) => it,
                _ => "",
            };
            if page_index.eq(format!("{}", i).as_str()) {
                self.cm.bvideo_info.write().await.current_page = i as usize;
                cid.push_str(p.pointer("/cid").ok_or(anyhow!("json err"))?.as_u64().unwrap().to_string().as_str());
                if i > 1 {
                    let t = title.clone();
                    title.clear();
                    title.push_str(format!("{} - {} - {}", &t, &i, &subtitle).as_str());
                }
            }
        }
        Ok((bvid, cid, title, artist))
    }

    pub async fn get_video(&self, page: usize) -> Result<Vec<String>> {
        let cookies = if self.cm.cookies_from_browser.is_empty() {
            self.cm.bcookie.clone()
        } else {
            crate::utils::cookies::get_cookies_from_browser(&self.cm.cookies_from_browser, "bilibili.com").await?
        };
        let mut ret: Vec<String> = Vec::new();
        if matches!(
            self.cm.bvideo_info.read().await.video_type,
            crate::config::config::BVideoType::Bangumi
        ) {
            let video_url = if page == 0 {
                let page = self.cm.bvideo_info.read().await.current_page;
                let u = self.cm.bvideo_info.read().await.base_url.clone();
                let _ = self.get_page_info_ep(&u).await?;
                let page = if page > self.cm.bvideo_info.read().await.plist.len() {
                    self.cm.bvideo_info.read().await.plist.len()
                } else {
                    page
                };
                format!(
                    "https://www.bilibili.com/bangumi/play/{}",
                    self.cm.bvideo_info.read().await.plist[page - 1]
                )
            } else {
                let page = if page > self.cm.bvideo_info.read().await.plist.len() {
                    self.cm.bvideo_info.read().await.plist.len()
                } else {
                    page
                };
                format!(
                    "https://www.bilibili.com/bangumi/play/{}",
                    self.cm.bvideo_info.read().await.plist[page - 1]
                )
            };
            let (bvid, cid, title, _artist, _season_type) = self.get_page_info_ep(&video_url).await?;
            ret.push(title);
            ret.push(cid.clone());
            let mut param1 = Vec::new();
            param1.push(("cid", cid.as_str()));
            param1.push(("bvid", bvid.as_str()));
            param1.push(("qn", "120"));
            param1.push(("otype", "json"));
            param1.push(("fourk", "1"));
            param1.push(("fnver", "0"));
            param1.push(("fnval", "16"));
            let client = reqwest::Client::builder()
                .user_agent(crate::utils::gen_ua())
                .connect_timeout(tokio::time::Duration::from_secs(10))
                .build()?;
            let resp = client
                .get(&self.apiv_ep)
                .header("Referer", video_url)
                .header("Cookie", cookies)
                .query(&param1)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            // println!("{:?}", &resp);
            let j = resp.pointer("/result").ok_or(anyhow!("get_video pje 1"))?;
            if j.pointer("/dash").is_some() {
                let dash_id = j
                    .pointer("/dash/video/0/id")
                    .ok_or(anyhow!("get_video pje 2"))?
                    .as_i64()
                    .ok_or(anyhow!("get_video ce 1"))?;
                if j.pointer("/dash/video")
                    .ok_or(anyhow!("get_video pje 3"))?
                    .as_array()
                    .ok_or(anyhow!("cannot convert to vec"))?
                    .len()
                    > 1
                    && dash_id
                        == j.pointer("/dash/video/0/id")
                            .ok_or(anyhow!("get_video pje 4"))?
                            .as_i64()
                            .ok_or(anyhow!(""))?
                {
                    if j.pointer("/dash/video/0/codecid")
                        .ok_or(anyhow!("get_video pje n"))?
                        .as_i64()
                        .ok_or(anyhow!(""))?
                        == 12
                    {
                        ret.push(
                            j.pointer("/dash/video/0/base_url")
                                .ok_or(anyhow!("get_video pje 7"))?
                                .as_str()
                                .ok_or(anyhow!(""))?
                                .to_string(),
                        );
                        ret.push(
                            j.pointer("/dash/audio/0/base_url")
                                .ok_or(anyhow!("get_video pje 6"))?
                                .as_str()
                                .ok_or(anyhow!(""))?
                                .to_string(),
                        );
                        ret.push(
                            j.pointer("/dash/video/1/base_url")
                                .ok_or(anyhow!("get_video pje 5"))?
                                .as_str()
                                .ok_or(anyhow!(""))?
                                .to_string(),
                        );
                    } else {
                        ret.push(
                            j.pointer("/dash/video/1/base_url")
                                .ok_or(anyhow!("get_video pje 10"))?
                                .as_str()
                                .ok_or(anyhow!(""))?
                                .to_string(),
                        );
                        ret.push(
                            j.pointer("/dash/audio/0/base_url")
                                .ok_or(anyhow!("get_video pje 9"))?
                                .as_str()
                                .ok_or(anyhow!(""))?
                                .to_string(),
                        );
                        ret.push(
                            j.pointer("/dash/video/0/base_url")
                                .ok_or(anyhow!("get_video pje 8"))?
                                .as_str()
                                .ok_or(anyhow!(""))?
                                .to_string(),
                        );
                    }
                } else {
                    ret.push(
                        j.pointer("/dash/video/0/base_url")
                            .ok_or(anyhow!("get_video pje 11"))?
                            .as_str()
                            .ok_or(anyhow!(""))?
                            .to_string(),
                    );
                    ret.push(
                        j.pointer("/dash/audio/0/base_url")
                            .ok_or(anyhow!("get_video pje 12"))?
                            .as_str()
                            .ok_or(anyhow!(""))?
                            .to_string(),
                    );
                }
            } else {
                let videos = j.pointer("/durl").ok_or(anyhow!("get_video pje 13"))?.as_array().ok_or(anyhow!(""))?;
                for v in videos {
                    ret.push(
                        v.pointer("url").ok_or(anyhow!("get_video pje 14"))?.as_str().ok_or(anyhow!(""))?.to_string(),
                    );
                }
            }
        } else {
            let video_url = if page == 0 {
                format!(
                    "{}?p={}",
                    self.cm.bvideo_info.read().await.base_url,
                    self.cm.bvideo_info.read().await.current_page
                )
            } else {
                let page = if page > self.cm.bvideo_info.read().await.plist.len() {
                    self.cm.bvideo_info.read().await.plist.len()
                } else {
                    page
                };
                format!("{}?p={}", self.cm.bvideo_info.read().await.base_url, page)
            };
            let (bvid, cid, title, artist) = self.get_page_info(&video_url).await?;
            println!("{} {} {} {}", &bvid, &cid, &title, &artist);
            ret.push(title);
            ret.push(cid.clone());
            let mut param1 = Vec::new();
            param1.push(("cid", cid.as_str()));
            param1.push(("bvid", bvid.as_str()));
            param1.push(("qn", "120"));
            param1.push(("otype", "json"));
            param1.push(("fourk", "1"));
            param1.push(("fnver", "0"));
            param1.push(("fnval", "16"));
            let client = reqwest::Client::builder()
                .user_agent(crate::utils::gen_ua())
                .connect_timeout(tokio::time::Duration::from_secs(10))
                .build()?;
            let resp = client
                .get(&self.apiv)
                .header("Referer", video_url)
                .header("Cookie", cookies)
                .query(&param1)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            let j = resp.pointer("/data").ok_or(anyhow!("get_video pje 15"))?;
            if j.pointer("/dash").is_some() {
                let dash_id = j.pointer("/dash/video/0/id").ok_or(anyhow!("get_video pje 16"))?.as_i64().unwrap();
                if j.pointer("/dash/video")
                    .ok_or(anyhow!("get_video pje 17"))?
                    .as_array()
                    .ok_or(anyhow!("cannot convert to vec"))?
                    .len()
                    > 1
                    && dash_id == j.pointer("/dash/video/0/id").ok_or(anyhow!("get_video pje 18"))?.as_i64().unwrap()
                {
                    if j.pointer("/dash/video/0/codecid").ok_or(anyhow!("get_video pje 19"))?.as_i64().unwrap() == 12 {
                        ret.push(
                            j.pointer("/dash/video/0/base_url")
                                .ok_or(anyhow!("get_video pje 22"))?
                                .as_str()
                                .unwrap()
                                .to_string(),
                        );
                        ret.push(
                            j.pointer("/dash/audio/0/base_url")
                                .ok_or(anyhow!("get_video pje 21"))?
                                .as_str()
                                .unwrap()
                                .to_string(),
                        );
                        ret.push(
                            j.pointer("/dash/video/1/base_url")
                                .ok_or(anyhow!("get_video pje 20"))?
                                .as_str()
                                .unwrap()
                                .to_string(),
                        );
                    } else {
                        ret.push(
                            j.pointer("/dash/video/1/base_url")
                                .ok_or(anyhow!("get_video pje 25"))?
                                .as_str()
                                .unwrap()
                                .to_string(),
                        );
                        ret.push(
                            j.pointer("/dash/audio/0/base_url")
                                .ok_or(anyhow!("get_video pje 24"))?
                                .as_str()
                                .unwrap()
                                .to_string(),
                        );
                        ret.push(
                            j.pointer("/dash/video/0/base_url")
                                .ok_or(anyhow!("get_video pje 23"))?
                                .as_str()
                                .unwrap()
                                .to_string(),
                        );
                    }
                } else {
                    ret.push(
                        j.pointer("/dash/video/0/base_url")
                            .ok_or(anyhow!("get_video pje 26"))?
                            .as_str()
                            .unwrap()
                            .to_string(),
                    );
                    ret.push(
                        j.pointer("/dash/audio/0/base_url")
                            .ok_or(anyhow!("get_video pje 27"))?
                            .as_str()
                            .unwrap()
                            .to_string(),
                    );
                }
            } else {
                let videos = j.pointer("/durl").ok_or(anyhow!("get_video pje 28"))?.as_array().unwrap();
                for v in videos {
                    ret.push(v.pointer("url").ok_or(anyhow!("get_video pje 29"))?.as_str().unwrap().to_string());
                }
            }
        }
        Ok(ret)
    }
}
