use log::{debug, info};
use regex::Regex;
use reqwest::Client;
use std::{collections::HashMap, rc::Rc};

use crate::{dmlerr, dmlive::DMLContext, utils};

const YTB_API1: &'static str = "https://www.youtube.com/youtubei/v1/player";

pub async fn get_live_info(
    client: &Client, room_url: &str,
) -> anyhow::Result<(String, String, String, bool, String, String, String)> {
    let resp = client
        .get(room_url)
        .header("Accept-Language", "en-US")
        .header("Connection", "keep-alive")
        .header("Referer", "https://www.youtube.com/")
        .send()
        .await?
        .text()
        .await?;

    let re_cover = Regex::new(r#"link\s+rel="image_src"\s+href="([^"]+)""#).unwrap();
    let re_owner = Regex::new(r#"meta\s+property="og:title"\s+content="([^"]+)""#).unwrap();
    let re_cid_new = Regex::new(r#"link\s+rel="alternate"[^>]+href="([^"]+)""#).unwrap();
    let avatar = re_cover.captures(&resp).and_then(|x| x.get(1)).map(|x| x.as_str());
    let owner = re_owner.captures(&resp).and_then(|x| x.get(1)).map(|x| x.as_str());
    let cid_new =
        re_cid_new.captures(&resp).and_then(|x| x.get(1)?.as_str().split('/').find_map(|x| x.strip_prefix("@")));

    let re = Regex::new(r"ytInitialPlayerResponse\s*=\s*(\{.+?\});.*?</script>").unwrap();
    let j: Option<serde_json::Value> =
        serde_json::from_str(re.captures(&resp).and_then(|x| x.get(1)).map_or("", |x| x.as_str())).ok();
    let j = j.as_ref();
    let owner = j.and_then(|x| x.pointer("/videoDetails/author")?.as_str()).or(owner).ok_or_else(|| dmlerr!())?;
    let title = j.and_then(|x| x.pointer("/videoDetails/title")?.as_str()).unwrap_or("没有直播标题");
    let cover = j
        .and_then(|x| x.pointer("/videoDetails/thumbnail/thumbnails")?.as_array()?.last()?.pointer("/url")?.as_str())
        .or(avatar)
        .ok_or_else(|| dmlerr!())?;
    let vid = j.and_then(|x| x.pointer("/videoDetails/videoId")?.as_str()).unwrap_or("");
    let cid = j.and_then(|x| x.pointer("/videoDetails/channelId")?.as_str()).unwrap_or("");
    let cid_new = j
        .and_then(|x| {
            x.pointer("/microformat/playerMicroformatRenderer/ownerProfileUrl")?
                .as_str()?
                .split('/')
                .last()?
                .strip_prefix("@")
        })
        .or(cid_new)
        .ok_or_else(|| dmlerr!())?;
    let is_live = j.and_then(|x| x.pointer("/videoDetails/isLive")?.as_bool()).unwrap_or(false);

    Ok((
        owner.to_string(),
        title.to_string(),
        cover.to_string(),
        is_live,
        cid_new.to_string(),
        vid.to_string(),
        cid.to_string(),
    ))
}

pub struct Youtube {
    ctx: Rc<DMLContext>,
}

impl Youtube {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Youtube { ctx }
    }

    #[allow(dead_code)]
    pub async fn decode_mpd(client: &Client, url: &str) -> anyhow::Result<HashMap<&'static str, String>> {
        info!("{url}");
        let mut ret = HashMap::new();
        let mut video_base_url = Vec::new();
        let mut audio_base_url = None;
        let mut sq = "";
        let resp = client
            .get(url)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Accept-Language", "en-US")
            .header("Referer", "https://www.youtube.com/")
            .send()
            .await?
            .text()
            .await?;
        let doc = roxmltree::Document::parse(resp.as_str())?;
        let elem_vs: Vec<roxmltree::Node> = doc
            .descendants()
            .filter(|n| {
                n.tag_name().name() == "AdaptationSet" && n.attribute("mimeType").unwrap_or("").contains("video")
            })
            .collect();
        let mut tmpnode = None;
        for elem_v in elem_vs {
            let mut url = None;
            for st in elem_v.descendants() {
                if st.has_attribute("bandwidth") {
                    debug!("{st:?}");
                    for e in st.descendants() {
                        if e.tag_name().name().eq("BaseURL") {
                            url = e.text();
                        }
                    }
                }
            }
            if url.is_some() {
                video_base_url.push(url.unwrap());
            }
        }
        let elem_a = doc
            .descendants()
            .find(|n| n.tag_name().name() == "AdaptationSet" && n.attribute("mimeType").unwrap_or("").contains("audio"))
            .unwrap();
        for e in elem_a.descendants() {
            if e.has_attribute("bandwidth") {
                tmpnode = Some(e);
            }
        }
        for e in tmpnode.unwrap().descendants() {
            if e.tag_name().name().eq("SegmentList") {
                for seg in e.descendants() {
                    if seg.tag_name().name().eq("SegmentURL") {
                        let spl: Vec<_> = seg.attribute("media").unwrap_or("").split('/').collect();
                        sq = spl[1];
                    }
                }
            }
            if e.tag_name().name().eq("BaseURL") {
                audio_base_url = e.text();
            }
        }

        if !video_base_url.is_empty() && audio_base_url.is_some() {
            ret.insert("url", video_base_url.last().unwrap().to_string());
            ret.insert("url_v", video_base_url.last().unwrap().to_string());
            ret.insert("url_a", audio_base_url.unwrap().to_string());
            ret.insert("sq", sq.to_string());
            Ok(ret)
        } else {
            Err(anyhow::anyhow!("no dash url found"))
        }
    }

    #[allow(dead_code)]
    pub async fn decode_m3u8(client: &Client, url: &str) -> anyhow::Result<String> {
        let resp = client
            .get(url)
            .header("User-Agent", crate::utils::gen_ua())
            .header("Accept-Language", "en-US")
            .header("Referer", "https://www.youtube.com/")
            .send()
            .await?
            .text()
            .await?;
        let mut m3u8_url = "";
        for l in resp.lines() {
            info!("{l}");
            if l.contains(".m3u8") {
                m3u8_url = l;
            }
        }
        if m3u8_url.is_empty() {
            Err(anyhow::anyhow!("no m3u8 url found"))
        } else {
            Ok(m3u8_url.to_string())
        }
    }

    pub async fn get_live(&self) -> anyhow::Result<HashMap<&'static str, String>> {
        let client = reqwest::Client::builder()
            .user_agent(utils::gen_ua())
            .timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let room_info = get_live_info(&client, &self.ctx.cm.room_url).await?;
        info!("{room_info:?}");
        room_info.3.then(|| 0).ok_or_else(|| dmlerr!())?;

        let vid = room_info.5.as_str();
        let payload = format!(
            r#"{{"videoId": "{vid}", "contentCheckOk": true, "racyCheckOk": true, "context": {{ "client": {{ "clientName": "ANDROID", "clientVersion": "19.45.36", "platform": "DESKTOP",   "clientScreen": "EMBED",   "clientFormFactor": "UNKNOWN_FORM_FACTOR",   "browserName": "Chrome",  }},   "user": {{"lockedSafetyMode": "false"}}, "request": {{"useSsl": "true"}}, }}, }}"#,
        );
        let resp = client
            .post(YTB_API1)
            .query(&[("key", "AIzaSyAO_FJ2SlqU8Q4STEHLGCilw_Y9_11qcW8")])
            .header("User-Agent", crate::utils::gen_ua())
            .header("Referer", "https://www.youtube.com")
            .body(payload)
            .send()
            .await?
            .json::<serde_json::Value>()
            .await?;

        let hls_url =
            resp.pointer("/streamingData/hlsManifestUrl").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;
        // let mpd_url =
        //     resp.pointer("/streamingData/dashManifestUrl").and_then(|x| x.as_str()).ok_or_else(|| dmlerr!())?;

        let mut ret = HashMap::new();
        ret.insert("url", Self::decode_m3u8(&client, &hls_url).await?);
        // let mut ret = Self::decode_mpd(&client, &mpd_url).await?;

        ret.insert("title", format!("{} - {}", room_info.1, room_info.0));
        ret.insert("vid", room_info.5);
        ret.insert("cid", room_info.6);
        info!("{ret:?}");
        Ok(ret)
    }
}
