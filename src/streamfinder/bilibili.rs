use crate::dmlerr;
use crate::{config::ConfigManager, utils::cookies::get_cookies_from_browser};
use anyhow::Result;
use log::info;
use regex::Regex;
use std::{collections::HashMap, rc::Rc};
use url::Url;

const BILI_API1: &'static str = "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo";
const BILI_API2: &'static str = "https://api.live.bilibili.com/xlive/web-room/v1/index/getInfoByRoom";
const BILI_API3: &'static str = "https://api.live.bilibili.com/room/v1/Room/playUrl";
const BILI_APIV: &'static str = "https://api.bilibili.com/x/player/playurl";
const BILI_APIV_EP: &'static str = "https://api.bilibili.com/pgc/player/web/playurl";

pub struct Bilibili {
    cm: Rc<ConfigManager>,
}

impl Bilibili {
    pub fn new(cm: Rc<ConfigManager>) -> Self {
        Bilibili { cm }
    }

    pub async fn get_live(&self, room_url: &str) -> Result<HashMap<String, String>> {
        let rid = Url::parse(room_url)?
            .path_segments()
            .ok_or_else(|| dmlerr!())?
            .last()
            .ok_or_else(|| dmlerr!())?
            .to_string();
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;

        let mut ret = HashMap::new();
        let mut param1 = Vec::new();

        param1.push(("room_id", rid.as_str()));
        let resp = client.get(BILI_API2).query(&param1).send().await?.json::<serde_json::Value>().await?;
        resp.pointer("/data/room_info/live_status")
            .ok_or_else(|| dmlerr!())?
            .as_i64()
            .ok_or_else(|| dmlerr!())?
            .eq(&1)
            .then(|| 0)
            .ok_or_else(|| dmlerr!())?;
        ret.insert(
            String::from("title"),
            format!(
                "{} - {}",
                resp.pointer("/data/room_info/title").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
                resp.pointer("/data/anchor_info/base_info/uname")
                    .ok_or_else(|| dmlerr!())?
                    .as_str()
                    .ok_or_else(|| dmlerr!())?
            ),
        );

        param1.clear();
        param1.push(("qn", "20000"));
        param1.push(("platform", "web"));
        param1.push(("cid", rid.as_str()));
        let resp = client
            .get(BILI_API3)
            .header("Referer", room_url)
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        info!("{}", &resp.to_string());
        let url = resp.pointer("/data/durl/0/url").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?;
        ret.insert(String::from("url"), url.to_string());

        Ok(ret)
    }

    #[allow(unused)]
    pub async fn get_live_new(&self, room_url: &str) -> Result<HashMap<String, String>> {
        let rid = Url::parse(room_url)?
            .path_segments()
            .ok_or_else(|| dmlerr!())?
            .last()
            .ok_or_else(|| dmlerr!())?
            .to_string();
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;

        let mut ret = HashMap::new();
        let mut param1 = Vec::new();
        // room_id=114514&protocol=0,1&format=0,1,2&codec=0,1,2&qn=10000&platform=web&ptype=8&dolby=5&panorama=1
        // param1.push(("no_playurl", "0"));
        // param1.push(("mask", "1"));
        param1.push(("room_id", rid.as_str()));
        param1.push(("protocol", "0,1"));
        param1.push(("format", "0,1,2"));
        param1.push(("codec", "0,1"));
        param1.push(("qn", "20000"));
        param1.push(("platform", "web"));
        param1.push(("ptype", "8"));
        param1.push(("dolby", "5"));
        param1.push(("panorama", "1"));

        let cookie = if self.cm.cookies_from_browser.is_empty() {
            self.cm.bcookie.clone()
        } else {
            get_cookies_from_browser(&self.cm.cookies_from_browser, ".bilibili.com").await?
        };
        let resp = client
            .get(BILI_API1)
            .header("Referer", room_url)
            .header("Cookie", cookie)
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        info!("{}", &resp.to_string());
        let j = resp.pointer("/data/playurl_info/playurl/stream/0/format/0/codec/0").ok_or_else(|| dmlerr!())?;
        ret.insert(
            String::from("url"),
            format!(
                "{}{}{}",
                j.pointer("/url_info/0/host").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
                j.pointer("/base_url").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
                j.pointer("/url_info/0/extra").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
            ),
        );
        param1.clear();
        param1.push(("room_id", rid.as_str()));
        let resp = client.get(BILI_API2).query(&param1).send().await?.json::<serde_json::Value>().await?;
        ret.insert(
            String::from("title"),
            format!(
                "{} - {}",
                resp.pointer("/data/room_info/title").ok_or_else(|| dmlerr!())?.as_str().ok_or_else(|| dmlerr!())?,
                resp.pointer("/data/anchor_info/base_info/uname")
                    .ok_or_else(|| dmlerr!())?
                    .as_str()
                    .ok_or_else(|| dmlerr!())?
            ),
        );
        Ok(ret)
    }

    pub async fn get_page_info_ep(
        &self, video_url: &str, mut page: usize,
    ) -> Result<(String, String, String, String, String)> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(video_url).header("Referer", video_url).send().await?.text().await?;
        let re = Regex::new(r"_NEXT_DATA_.+>\s*(\{.+\})\s*<").unwrap();
        let j: serde_json::Value = serde_json::from_str(re.captures(&resp).ok_or_else(|| dmlerr!())?[1].as_ref())?;
        // println!("{:?}", &j);
        let j = j
            .pointer("/props/pageProps/dehydratedState/queries/0/state/data/seasonInfo/mediaInfo")
            .ok_or_else(|| dmlerr!())?;
        let season_type = j.pointer("/season_type").ok_or_else(|| dmlerr!())?.as_i64().unwrap().to_string();
        let eplist = j.pointer("/episodes").ok_or_else(|| dmlerr!())?.as_array().unwrap();
        let epid = url::Url::parse(video_url)?
            .path_segments()
            .ok_or_else(|| dmlerr!())?
            .last()
            .ok_or_else(|| dmlerr!())?
            .to_string();
        if page == 0 {
            page = 1;
            if epid.starts_with("ep") {
                for (i, e) in eplist.iter().enumerate() {
                    if e.pointer("/link").ok_or_else(|| dmlerr!())?.as_str().unwrap().contains(&epid) {
                        page = i + 1;
                        break;
                    }
                }
            }
        }
        let ep = match eplist.get(page - 1) {
            Some(ep) => ep,
            None => {
                page = eplist.len();
                eplist.last().ok_or_else(|| dmlerr!())?
            }
        };
        self.cm.bvideo_info.borrow_mut().current_page = page;

        let bvid = ep.pointer("/bvid").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        let cid = ep.pointer("/cid").ok_or_else(|| dmlerr!())?.as_i64().unwrap().to_string();
        let subtitle = ep.pointer("/long_title").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        let title = j.pointer("/title").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        let title_number = ep.pointer("/titleFormat").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        let referer = ep.pointer("/link").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();

        Ok((
            bvid,
            cid,
            format!("{} - {} - {}: {}", &title, page, &title_number, &subtitle),
            referer,
            season_type,
        ))
    }

    pub async fn get_page_info(&self, video_url: &str, mut page: usize) -> Result<(String, String, String, String)> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(video_url).header("Referer", video_url).send().await?;
        let resp = resp.text().await?;
        let re = Regex::new(r"__INITIAL_STATE__=(\{.+?\});").unwrap();
        let j: serde_json::Value =
            serde_json::from_str(re.captures(&resp).ok_or_else(|| dmlerr!())?[1].to_string().as_ref())?;
        let bvid = j.pointer("/videoData/bvid").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        let title = j.pointer("/videoData/title").ok_or_else(|| dmlerr!())?.as_str().unwrap();
        let artist = j.pointer("/videoData/owner/name").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        let j = j.pointer("/videoData/pages").ok_or_else(|| dmlerr!())?.as_array().unwrap();
        if page == 0 {
            page = 1;
        }
        let p = match j.get(page - 1) {
            Some(p) => p,
            None => {
                page = j.len();
                j.last().ok_or_else(|| dmlerr!())?
            }
        };
        self.cm.bvideo_info.borrow_mut().current_page = page;

        let cid = p.pointer("/cid").ok_or_else(|| dmlerr!())?.as_u64().unwrap().to_string();
        let final_title = if j.len() == 1 {
            format!("{} - {}", &title, &artist)
        } else {
            let subtitle = p.pointer("/part").ok_or_else(|| dmlerr!())?.as_str().unwrap();
            format!("{} - {} - {} - {}", &title, page, &subtitle, &artist)
        };

        Ok((bvid, cid, final_title, artist))
    }

    pub async fn get_video(&self, page: usize) -> Result<Vec<String>> {
        let cookies = if self.cm.cookies_from_browser.is_empty() {
            self.cm.bcookie.clone()
        } else {
            get_cookies_from_browser(&self.cm.cookies_from_browser, ".bilibili.com").await?
        };
        let mut ret: Vec<String> = Vec::new();
        if matches!(
            self.cm.bvideo_info.borrow().video_type,
            crate::config::config::BVideoType::Bangumi
        ) {
            let u = self.cm.bvideo_info.borrow().base_url.clone();
            let (bvid, cid, title, referer, _season_type) = self.get_page_info_ep(&u, page).await?;
            ret.push(title);
            ret.push(cid.clone());
            let mut param1 = Vec::new();
            param1.push(("cid", cid.as_str()));
            param1.push(("bvid", bvid.as_str()));
            // param1.push(("qn", "126"));
            // param1.push(("fourk", "1"));
            // param1.push(("fnver", "0"));
            param1.push(("fnval", "3024"));
            let client = reqwest::Client::builder()
                .user_agent(crate::utils::gen_ua_safari())
                .connect_timeout(tokio::time::Duration::from_secs(10))
                .build()?;
            let resp = client
                .get(BILI_APIV_EP)
                .header("Referer", &referer)
                .header("Cookie", cookies)
                .query(&param1)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            // println!("{:?}", &resp);
            let j = resp.pointer("/result").ok_or_else(|| dmlerr!())?;
            let mut videos = HashMap::new();
            let mut audios = HashMap::new();
            for ele in j.pointer("/dash/video").ok_or_else(|| dmlerr!())?.as_array().unwrap() {
                let mut id = ele.pointer("/id").ok_or_else(|| dmlerr!())?.as_u64().unwrap() * 10;
                if ele.pointer("/codecid").ok_or_else(|| dmlerr!())?.as_u64().eq(&Some(7)) {
                    id += 1;
                }
                videos.insert(
                    id,
                    ele.pointer("/base_url").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                );
            }
            for ele in j.pointer("/dash/audio").ok_or_else(|| dmlerr!())?.as_array().unwrap() {
                audios.insert(
                    ele.pointer("/id").ok_or_else(|| dmlerr!())?.as_u64().unwrap(),
                    ele.pointer("/base_url").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                );
            }
            if let Some(ele) = j.pointer("/dash/flac/audio") {
                audios.insert(
                    ele.pointer("/id").ok_or_else(|| dmlerr!())?.as_u64().unwrap() + 100,
                    ele.pointer("/base_url").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                );
            }
            ret.push(videos.iter().max_by_key(|x| x.0).unwrap().1.to_string());
            ret.push(audios.iter().max_by_key(|x| x.0).unwrap().1.to_string());
        } else {
            let u = self.cm.bvideo_info.borrow().base_url.clone();
            let (bvid, cid, title, _artist) = self.get_page_info(&u, page).await?;
            // println!("{} {} {} {}", &bvid, &cid, &title, &artist);
            ret.push(title);
            ret.push(cid.clone());
            let mut param1 = Vec::new();
            param1.push(("cid", cid.as_str()));
            param1.push(("bvid", bvid.as_str()));
            param1.push(("fnval", "3024"));
            if cookies.is_empty() {
                param1.push(("try_look", "1"));
            }
            let client = reqwest::Client::builder()
                .user_agent(crate::utils::gen_ua_safari())
                .connect_timeout(tokio::time::Duration::from_secs(10))
                .build()?;
            let resp = client
                .get(BILI_APIV)
                .header("Cookie", cookies)
                .query(&param1)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            let j = resp.pointer("/data").ok_or_else(|| dmlerr!())?;
            let mut videos = HashMap::new();
            let mut audios = HashMap::new();
            for ele in j.pointer("/dash/video").ok_or_else(|| dmlerr!())?.as_array().unwrap() {
                let mut id = ele.pointer("/id").ok_or_else(|| dmlerr!())?.as_u64().unwrap() * 10;
                if ele.pointer("/codecid").ok_or_else(|| dmlerr!())?.as_u64().eq(&Some(7)) {
                    id += 1;
                }
                videos.insert(
                    id,
                    ele.pointer("/base_url").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                );
            }
            for ele in j.pointer("/dash/audio").ok_or_else(|| dmlerr!())?.as_array().unwrap() {
                audios.insert(
                    ele.pointer("/id").ok_or_else(|| dmlerr!())?.as_u64().unwrap(),
                    ele.pointer("/base_url").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                );
            }
            if let Some(ele) = j.pointer("/dash/flac/audio") {
                audios.insert(
                    ele.pointer("/id").ok_or_else(|| dmlerr!())?.as_u64().unwrap() + 100,
                    ele.pointer("/base_url").ok_or_else(|| dmlerr!())?.as_str().unwrap(),
                );
            }
            ret.push(videos.iter().max_by_key(|x| x.0).unwrap().1.to_string());
            ret.push(audios.iter().max_by_key(|x| x.0).unwrap().1.to_string());
        }
        Ok(ret)
    }
}
