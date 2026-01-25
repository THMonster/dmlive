use crate::dmlerr;
use crate::dmlive::DMLContext;
use crate::utils::cookies::get_cookies_from_browser;
use anyhow::Result;
use log::info;
use regex::Regex;
use std::{collections::HashMap, rc::Rc};

const BILI_API1: &'static str = "https://api.live.bilibili.com/xlive/web-room/v2/index/getRoomPlayInfo";
const BILI_API2: &'static str = "https://api.live.bilibili.com/xlive/web-room/v1/index/getRoomBaseInfo";
const BILI_API3: &'static str = "https://api.live.bilibili.com/room/v1/Room/playUrl";
const BILI_APIV: &'static str = "https://api.bilibili.com/x/player/wbi/playurl";
// const BILI_APIV_EP: &'static str = "https://api.bilibili.com/pgc/player/web/playurl";
const BILI_APIV_EP_LIST: &'static str = "https://api.bilibili.com/pgc/view/web/ep/list";

pub async fn get_live_info(client: &reqwest::Client, rid: &str) -> anyhow::Result<(String, String, String, bool)> {
    let mut param1 = Vec::new();
    param1.push(("room_ids", rid));
    param1.push(("req_biz", "web_room_componet"));
    let resp = client
        .get(BILI_API2)
        .query(&param1)
        .header("User-Agent", crate::utils::gen_ua())
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;
    let j = resp.pointer("/data/by_room_ids").and_then(|x| x.as_object()).ok_or_else(|| dmlerr!())?;
    let j = j.iter().next().ok_or_else(|| dmlerr!())?.1;
    let title = j.pointer("/title").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
    let uname = j.pointer("/uname").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
    let bg = j.pointer("/background").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
    let cover = j.pointer("/cover").and_then(|x| x.as_str()).unwrap_or(bg);
    let is_living = j.pointer("/live_status").and_then(|x| x.as_i64()).ok_or_else(|| dmlerr!())?;
    Ok((
        uname.to_string(),
        title.to_string(),
        cover.to_string(),
        if is_living == 1 { true } else { false },
    ))
}

pub struct Bilibili {
    ctx: Rc<DMLContext>,
}

impl Bilibili {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Bilibili { ctx }
    }

    pub async fn get_live(&self) -> Result<()> {
        let rid = self.ctx.cm.room_id.as_str();
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;

        let mut param1 = Vec::new();

        let room_info = get_live_info(&client, rid).await?;
        room_info.3.then(|| 0).ok_or_else(|| dmlerr!())?;

        param1.clear();
        param1.push(("qn", "20000"));
        param1.push(("platform", "web"));
        param1.push(("cid", rid));
        // let resp = client
        //     .get(BILI_API3)
        //     .header("Referer", room_url)
        //     .query(&param1)
        //     .send()
        //     .await?
        //     .json::<serde_json::Value>()
        //     .await?;
        // info!("{resp:?}");
        // let url = match resp.pointer("/data/durl/0/url").and_then(|x| x.as_str()) {
        //     Some(it) => it.to_string(),
        //     None => self.get_live_new(room_url).await?,
        // };
        let url = self.get_live_new().await?;
        let mut si = self.ctx.cm.stream_info.borrow_mut();
        si.insert("owner_name", room_info.0);
        si.insert("title", room_info.1);
        si.insert("cover", room_info.2);
        si.insert("url", url);
        Ok(())
    }

    #[allow(unused)]
    pub async fn get_live_new(&self) -> Result<String> {
        // pub async fn get_live_new(&self, room_url: &str) -> Result<HashMap<&'static str, String>> {
        let rid = self.ctx.cm.room_id.as_str();
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;

        let mut param1 = Vec::new();
        // room_id=114514&protocol=0,1&format=0,1,2&codec=0,1,2&qn=10000&platform=web&ptype=8&dolby=5&panorama=1
        // param1.push(("no_playurl", "0"));
        // param1.push(("mask", "1"));
        param1.push(("room_id", rid));
        param1.push(("protocol", "0,1"));
        param1.push(("format", "0,1,2"));
        param1.push(("codec", "0,1"));
        param1.push(("qn", "20000"));
        param1.push(("platform", "web"));
        param1.push(("ptype", "8"));
        param1.push(("dolby", "5"));
        param1.push(("panorama", "1"));

        let cookie = if self.ctx.cm.cookies_from_browser.is_empty() {
            self.ctx.cm.bcookie.clone()
        } else {
            get_cookies_from_browser(&self.ctx.cm.cookies_from_browser, ".bilibili.com").await?
        };
        let resp = client
            .get(BILI_API1)
            .header("Referer", &self.ctx.cm.room_url)
            .header("Cookie", cookie)
            .query(&param1)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;
        info!("{}", &resp.to_string());
        let j = resp.pointer("/data/playurl_info/playurl/stream/0/format/0/codec/0").ok_or_else(|| dmlerr!())?;
        return Ok(format!(
            "{}{}{}",
            j.pointer("/url_info/0/host").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?,
            j.pointer("/base_url").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?,
            j.pointer("/url_info/0/extra").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?
        ));
    }

    pub async fn get_page_info_ep(&self) -> Result<(String, String, String, String)> {
        let mut page = self.ctx.cm.bvideo_info.borrow().current_page;
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua_safari())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let epid = self.ctx.cm.room_id.as_str();
        let mut param1 = Vec::new();
        if epid.starts_with("ep") {
            param1.push(("ep_id", epid.replace("ep", "")));
        } else {
            param1.push(("season_id", epid.replace("ss", "")));
        }
        let resp = client.get(BILI_APIV_EP_LIST).query(&param1).send().await?.json::<serde_json::Value>().await?;

        let eplist = resp.pointer("/result/episodes").and_then(|x| x.as_array()).ok_or_else(|| dmlerr!())?;
        if page == 0 {
            page = 1;
            if epid.starts_with("ep") {
                for (i, e) in eplist.iter().enumerate() {
                    if e.pointer("/link").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?.contains(&epid) {
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
        self.ctx.cm.bvideo_info.borrow_mut().last_page = eplist.len();
        self.ctx.cm.bvideo_info.borrow_mut().current_page = page;

        let bvid = ep.pointer("/bvid").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?.to_string();
        let cid = ep.pointer("/cid").and_then(|x| x.as_i64()).ok_or_else(|| dmlerr!())?.to_string();
        let title = ep.pointer("/share_copy").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?.to_string();
        let link = ep.pointer("/link").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?.to_string();

        Ok((bvid, cid, format!("{title} - {page}"), link))
    }

    pub async fn get_page_info(&self, html: &str) -> Result<(String, String, String, String)> {
        let mut page = self.ctx.cm.bvideo_info.borrow().current_page;
        let re = Regex::new(r"__INITIAL_STATE__=(\{.+?\});").unwrap();
        let j: serde_json::Value =
            serde_json::from_str(re.captures(html).and_then(|x| x.get(1)).ok_or_else(|| dmlerr!())?.as_str())?;
        let bvid = j.pointer("/videoData/bvid").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let title = j.pointer("/videoData/title").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let artist = j.pointer("/videoData/owner/name").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        let j = j.pointer("/videoData/pages").and_then(|x| x.as_array()).ok_or_else(|| dmlerr!())?;
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
        self.ctx.cm.bvideo_info.borrow_mut().last_page = j.len();
        self.ctx.cm.bvideo_info.borrow_mut().current_page = page;

        let cid = p.pointer("/cid").and_then(|x| x.as_u64()).ok_or_else(|| dmlerr!())?;
        let final_title = if j.len() == 1 {
            format!("{title} - {artist}")
        } else {
            let subtitle = p.pointer("/part").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
            format!("{title} - {page} - {subtitle} - {artist}")
        };

        Ok((
            bvid.to_string(),
            cid.to_string(),
            final_title,
            artist.to_string(),
        ))
    }

    pub async fn get_video(&self) -> Result<()> {
        let f1 = |j: &serde_json::Value, ret: &mut HashMap<_, _>| -> _ {
            let mut videos = HashMap::new();
            let mut audios = HashMap::new();
            for ele in j.pointer("/dash/video").and_then(|x| x.as_array()).ok_or_else(|| dmlerr!())? {
                let mut id = ele.pointer("/id").and_then(|x| x.as_u64()).ok_or_else(|| dmlerr!())? * 10;
                if ele.pointer("/codecid").and_then(|x| x.as_u64()).ok_or_else(|| dmlerr!())?.eq(&7) {
                    id += 1;
                }
                let mut ul = Vec::new();
                ul.push(ele.pointer("/base_url").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?);
                ele.pointer("/backup_url")
                    .and_then(|x| x.as_array())
                    .ok_or_else(|| dmlerr!())?
                    .iter()
                    .for_each(|x| ul.push(x.as_str().unwrap()));
                videos.insert(
                    id,
                    ul.iter().find(|&&x| !x.contains("mcdn")).ok_or_else(|| dmlerr!())?.to_string(),
                );
            }
            for ele in j.pointer("/dash/audio").and_then(|x| x.as_array()).ok_or_else(|| dmlerr!())? {
                let mut ul = Vec::new();
                ul.push(ele.pointer("/base_url").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?);
                ele.pointer("/backup_url")
                    .and_then(|x| x.as_array())
                    .ok_or_else(|| dmlerr!())?
                    .iter()
                    .for_each(|x| ul.push(x.as_str().unwrap()));
                audios.insert(
                    ele.pointer("/id").and_then(|x| x.as_u64()).ok_or_else(|| dmlerr!())?,
                    ul.iter().find(|&&x| !x.contains("mcdn")).ok_or_else(|| dmlerr!())?.to_string(),
                );
            }
            if let Some(ele) = j.pointer("/dash/flac/audio") {
                let mut ul = Vec::new();
                ul.push(ele.pointer("/base_url").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?);
                ele.pointer("/backup_url")
                    .and_then(|x| x.as_array())
                    .ok_or_else(|| dmlerr!())?
                    .iter()
                    .for_each(|x| ul.push(x.as_str().unwrap()));
                audios.insert(
                    ele.pointer("/id").and_then(|x| x.as_u64()).ok_or_else(|| dmlerr!())? + 100,
                    ul.iter().find(|&&x| !x.contains("mcdn")).ok_or_else(|| dmlerr!())?.to_string(),
                );
            }
            ret.insert(
                "url",
                videos.iter().max_by_key(|x| x.0).unwrap().1.to_string(),
            );
            ret.insert(
                "url_v",
                videos.iter().max_by_key(|x| x.0).unwrap().1.to_string(),
            );
            ret.insert(
                "url_a",
                audios.iter().max_by_key(|x| x.0).unwrap().1.to_string(),
            );
            anyhow::Ok(())
        };

        let cookies = if self.ctx.cm.cookies_from_browser.is_empty() {
            self.ctx.cm.bcookie.clone()
        } else {
            get_cookies_from_browser(&self.ctx.cm.cookies_from_browser, ".bilibili.com").await?
        };
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua_safari())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;

        if matches!(
            self.ctx.cm.bvideo_info.borrow().video_type,
            crate::config::config::BVideoType::Bangumi
        ) {
            let (_bvid, cid, title, link) = self.get_page_info_ep().await?;

            let resp =
                client.get(&link).header("Referer", &link).header("Cookie", cookies).send().await?.text().await?;
            let re = Regex::new(r"const\s*playurlSSRData\s*=\s*(\{.+\})").unwrap();
            let j: serde_json::Value =
                serde_json::from_str(re.captures(&resp).and_then(|x| x.get(1)).ok_or_else(|| dmlerr!())?.as_str())?;
            let j = j.pointer("/data/result/video_info").ok_or_else(|| dmlerr!())?;

            let mut si = self.ctx.cm.stream_info.borrow_mut();
            self.ctx.cm.bvideo_info.borrow_mut().current_cid = cid;
            si.insert("title", title);
            f1(&j, &mut si)?;
        } else {
            let mut param1 = Vec::new();
            let p = if self.ctx.cm.bvideo_info.borrow().current_page == 0 {
                1
            } else {
                self.ctx.cm.bvideo_info.borrow().current_page
            };
            param1.push(("p", p));
            let resp = client
                .get(&self.ctx.cm.bvideo_info.borrow().base_url)
                .header("Cookie", &cookies)
                .query(&param1)
                .send()
                .await?
                .text()
                .await?;
            let (bvid, cid, title, _artist) = self.get_page_info(&resp).await?;
            // println!("{} {} {} {}", &bvid, &cid, &title, &artist);
            // let re = Regex::new(r"window.__playinfo__\s*=\s*(\{.+?\})\s*</script>").unwrap();
            // let j: serde_json::Value =
            //     serde_json::from_str(re.captures(&resp).ok_or_else(|| dmlerr!())?[1].to_string().as_ref())?;
            let keys = crate::utils::bili_wbi::get_wbi_keys(&cookies).await?;
            let params2 = vec![
                ("bvid", bvid),
                ("cid", cid.clone()),
                ("qn", String::from("0")),
                ("fnval", String::from("848")),
                ("fnver", String::from("0")),
                ("fourk", String::from("1")),
            ];
            let query = crate::utils::bili_wbi::encode_wbi(params2, keys);

            let j = client
                .get(format!("{BILI_APIV}?{query}"))
                .header("Cookie", &cookies)
                .send()
                .await?
                .json::<serde_json::Value>()
                .await?;
            let j = j.pointer("/data").ok_or_else(|| dmlerr!())?;

            let mut si = self.ctx.cm.stream_info.borrow_mut();
            si.insert("title", title);
            self.ctx.cm.bvideo_info.borrow_mut().current_cid = cid;
            f1(&j, &mut si)?;
        }
        Ok(())
    }
}
