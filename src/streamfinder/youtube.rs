use log::{debug, info};
use regex::Regex;
use reqwest::Client;
use std::collections::HashMap;

use crate::{dmlerr, utils};

pub struct Youtube {}

impl Youtube {
    pub fn new() -> Self {
        Youtube {}
    }

    #[allow(dead_code)]
    pub async fn decode_mpd(client: &Client, url: &str) -> anyhow::Result<HashMap<&'static str, String>> {
        info!("{}", url);
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
                    debug!("{:?}", &st);
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
            info!("{}", l);
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

    pub async fn get_live_info(
        client: &Client, room_url: &str,
    ) -> anyhow::Result<(String, String, String, bool, String, String)> {
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
        let avatar = re_cover.captures(&resp).and_then(|x| x.get(1)).map(|x| x.as_str());
        let owner = re_owner.captures(&resp).and_then(|x| x.get(1)).map(|x| x.as_str());

        let re = Regex::new(r"ytInitialPlayerResponse\s*=\s*(\{.+?\});.*?</script>").unwrap();
        let j: Option<serde_json::Value> =
            serde_json::from_str(re.captures(&resp).and_then(|x| x.get(1)).map_or("", |x| x.as_str())).ok();
        let j = j.as_ref();
        let owner = j.and_then(|x| x.pointer("/videoDetails/author")?.as_str()).or(owner).ok_or_else(|| dmlerr!())?;
        let title = j.and_then(|x| x.pointer("/videoDetails/title")?.as_str()).unwrap_or("没有直播标题");
        let cover = j
            .and_then(|x| {
                x.pointer("/videoDetails/thumbnail/thumbnails")?.as_array()?.last()?.pointer("/url")?.as_str()
            })
            .or(avatar)
            .ok_or_else(|| dmlerr!())?;
        let cid = j
            .and_then(|x| {
                x.pointer("/microformat/playerMicroformatRenderer/ownerProfileUrl")?
                    .as_str()?
                    .split('/')
                    .last()?
                    .strip_prefix("@")
            })
            .unwrap_or("");
        let is_live = j.and_then(|x| x.pointer("/videoDetails/isLive")?.as_bool()).unwrap_or(false);

        let mpd_url = j.and_then(|x| x.pointer("/streamingData/dashManifestUrl")?.as_str()).unwrap_or("");
        // let hls_url = j.pointer("/streamingData/hlsManifestUrl").ok_or_else(|| dmlerr!())?.as_str().unwrap();

        Ok((
            owner.to_string(),
            title.to_string(),
            cover.to_string(),
            is_live,
            cid.to_string(),
            mpd_url.to_string(),
        ))
    }

    pub async fn get_live(&self, room_url: &str) -> anyhow::Result<HashMap<&'static str, String>> {
        let client = reqwest::Client::builder()
            .user_agent(utils::gen_ua())
            .timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let url = url::Url::parse(room_url)?;
        let room_url = if url.as_str().contains("youtube.com/@") {
            let cid = url
                .path_segments()
                .and_then(|x| x.last().and_then(|x| x.strip_prefix("@")))
                .ok_or_else(|| dmlerr!())?;
            format!("https://www.youtube.com/@{}/live", &cid)
        } else {
            let vid = url.query_pairs().find(|q| q.0.eq("v")).unwrap().1;
            format!("https://www.youtube.com/watch?v={}", &vid)
        };

        let room_info = Self::get_live_info(&client, &room_url).await?;
        info!("{:?}", &room_info);
        room_info.3.then(|| 0).ok_or_else(|| dmlerr!())?;

        // let urls = self.decode_m3u8(&client, &hls_url).await?;
        let mut ret = Self::decode_mpd(&client, &room_info.5).await?;

        ret.insert("title", format!("{} - {}", room_info.1, room_info.0));
        ret.insert("room_url", room_url);
        Ok(ret)
    }
}
