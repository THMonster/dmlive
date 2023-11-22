use crate::config::ConfigManager;
use anyhow::anyhow;
use anyhow::Result;
use log::warn;
use regex::Regex;
use std::{collections::HashMap, sync::Arc};
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
        param1.push(("qn", "20000"));
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
        // warn!("{:?}", &resp);
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
    pub async fn get_page_info_ep(
        &self,
        video_url: &str,
        mut page: usize,
    ) -> Result<(String, String, String, String, String)> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let resp = client.get(video_url).header("Referer", video_url).send().await?.text().await?;
        let re = Regex::new(r"_NEXT_DATA_.+>\s*(\{.+\})\s*<").unwrap();
        let j: serde_json::Value = serde_json::from_str(re.captures(&resp).ok_or(anyhow!("gpie err a1"))?[1].as_ref())?;
        // println!("{:?}", &j);
        let j = j
            .pointer("/props/pageProps/dehydratedState/queries/0/state/data/mediaInfo")
            .ok_or(anyhow!("gpie err a2"))?;
        let season_type = j.pointer("/season_type").ok_or(anyhow!("gpie err b1"))?.as_i64().unwrap().to_string();
        let eplist = j.pointer("/episodes").ok_or(anyhow!("gpie err b2"))?.as_array().unwrap();
        let epid = url::Url::parse(video_url)?
            .path_segments()
            .ok_or(anyhow!("gpie err b3"))?
            .last()
            .ok_or(anyhow!("gpie err b4"))?
            .to_string();
        if page == 0 {
            page = 1;
            if epid.starts_with("ep") {
                for (i, e) in eplist.iter().enumerate() {
                    if e.pointer("/link").ok_or(anyhow!("gpie err c1"))?.as_str().unwrap().contains(&epid) {
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
                eplist.last().ok_or(anyhow!("gpie err d1"))?
            }
        };
        self.cm.bvideo_info.write().await.current_page = page;

        let bvid = ep.pointer("/bvid").ok_or(anyhow!("gpie err d2"))?.as_str().unwrap().to_string();
        let cid = ep.pointer("/cid").ok_or(anyhow!("gpie err d3"))?.as_i64().unwrap().to_string();
        let subtitle = ep.pointer("/long_title").ok_or(anyhow!("gpie err d4"))?.as_str().unwrap().to_string();
        let title = j.pointer("/title").ok_or(anyhow!("gpie err d5"))?.as_str().unwrap().to_string();
        let title_number = ep.pointer("/titleFormat").ok_or(anyhow!("gpie err d6"))?.as_str().unwrap().to_string();
        let referer = ep.pointer("/link").ok_or(anyhow!("gpie err d7"))?.as_str().unwrap().to_string();

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
            serde_json::from_str(re.captures(&resp).ok_or(anyhow!("gpi err a1"))?[1].to_string().as_ref())?;
        let bvid = j.pointer("/videoData/bvid").ok_or(anyhow!("json err"))?.as_str().unwrap().to_string();
        let title = j.pointer("/videoData/title").ok_or(anyhow!("json err"))?.as_str().unwrap();
        let artist = j.pointer("/videoData/owner/name").ok_or(anyhow!("json err"))?.as_str().unwrap().to_string();
        let j = j.pointer("/videoData/pages").ok_or(anyhow!("json err"))?.as_array().unwrap();
        if page == 0 {
            page = 1;
        }
        let p = match j.get(page - 1) {
            Some(p) => p,
            None => {
                page = j.len();
                j.last().ok_or(anyhow!("gpi err c1"))?
            }
        };
        self.cm.bvideo_info.write().await.current_page = page;

        let cid = p.pointer("/cid").ok_or(anyhow!("gpi err c2"))?.as_u64().unwrap().to_string();
        let final_title = if j.len() == 1 {
            format!("{} - {}", &title, &artist)
        } else {
            let subtitle = p.pointer("/part").ok_or(anyhow!("gpi err c3"))?.as_str().unwrap();
            format!("{} - {} - {} - {}", &title, page, &subtitle, &artist)
        };

        Ok((bvid, cid, final_title, artist))
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
            let u = self.cm.bvideo_info.read().await.base_url.clone();
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
                .get(&self.apiv_ep)
                .header("Referer", &referer)
                .header("Cookie", cookies)
                .query(&param1)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            // println!("{:?}", &resp);
            let j = resp.pointer("/result").ok_or(anyhow!("gv err b1"))?;
            let mut videos = HashMap::new();
            let mut audios = HashMap::new();
            for ele in j.pointer("/dash/video").ok_or(anyhow!("gv err b2"))?.as_array().unwrap() {
                let mut id = ele.pointer("/id").ok_or(anyhow!("gv err k3"))?.as_u64().unwrap() * 10;
                if ele.pointer("/codecid").ok_or(anyhow!("gv err k31"))?.as_u64().eq(&Some(7)) {
                    id += 1;
                }
                videos.insert(
                    id,
                    ele.pointer("/base_url").ok_or(anyhow!("gv err b4"))?.as_str().unwrap(),
                );
            }
            for ele in j.pointer("/dash/audio").ok_or(anyhow!("gv err b5"))?.as_array().unwrap() {
                audios.insert(
                    ele.pointer("/id").ok_or(anyhow!("gv err b6"))?.as_u64().unwrap(),
                    ele.pointer("/base_url").ok_or(anyhow!("gv err b7"))?.as_str().unwrap(),
                );
            }
            if let Some(ele) = j.pointer("/dash/flac/audio") {
                audios.insert(
                    ele.pointer("/id").ok_or(anyhow!("gv err c1"))?.as_u64().unwrap() + 100,
                    ele.pointer("/base_url").ok_or(anyhow!("gv err c2"))?.as_str().unwrap(),
                );
            }
            ret.push(videos.iter().max_by_key(|x| x.0).unwrap().1.to_string());
            ret.push(audios.iter().max_by_key(|x| x.0).unwrap().1.to_string());
        } else {
            let u = self.cm.bvideo_info.read().await.base_url.clone();
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
                .get(&self.apiv)
                .header("Cookie", cookies)
                .query(&param1)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            let j = resp.pointer("/data").ok_or(anyhow!("gv err k1"))?;
            let mut videos = HashMap::new();
            let mut audios = HashMap::new();
            for ele in j.pointer("/dash/video").ok_or(anyhow!("gv err k2"))?.as_array().unwrap() {
                let mut id = ele.pointer("/id").ok_or(anyhow!("gv err k3"))?.as_u64().unwrap() * 10;
                if ele.pointer("/codecid").ok_or(anyhow!("gv err k31"))?.as_u64().eq(&Some(7)) {
                    id += 1;
                }
                videos.insert(
                    id,
                    ele.pointer("/base_url").ok_or(anyhow!("gv err k4"))?.as_str().unwrap(),
                );
            }
            for ele in j.pointer("/dash/audio").ok_or(anyhow!("gv err k5"))?.as_array().unwrap() {
                audios.insert(
                    ele.pointer("/id").ok_or(anyhow!("gv err k6"))?.as_u64().unwrap(),
                    ele.pointer("/base_url").ok_or(anyhow!("gv err k7"))?.as_str().unwrap(),
                );
            }
            if let Some(ele) = j.pointer("/dash/flac/audio") {
                audios.insert(
                    ele.pointer("/id").ok_or(anyhow!("gv err l1"))?.as_u64().unwrap() + 100,
                    ele.pointer("/base_url").ok_or(anyhow!("gv err l2"))?.as_str().unwrap(),
                );
            }
            ret.push(videos.iter().max_by_key(|x| x.0).unwrap().1.to_string());
            ret.push(audios.iter().max_by_key(|x| x.0).unwrap().1.to_string());
        }
        Ok(ret)
    }
}
