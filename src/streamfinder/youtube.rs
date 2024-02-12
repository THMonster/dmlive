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
    async fn decode_mpd(&self, client: &Client, url: &str) -> anyhow::Result<String> {
        info!("{}", url);
        let mut dash_urls = "".to_owned();
        let mut video_base_url = Vec::new();
        let mut audio_base_url = None;
        let mut sq = 0;
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
                        sq = spl[1].parse()?;
                    }
                }
            }
            if e.tag_name().name().eq("BaseURL") {
                audio_base_url = e.text();
            }
        }

        if !video_base_url.is_empty() && audio_base_url.is_some() {
            dash_urls.push_str(video_base_url.last().unwrap());
            dash_urls.push('\n');
            dash_urls.push_str(audio_base_url.unwrap());
            dash_urls.push('\n');
            dash_urls.push_str(format!("{}", &sq).as_str());
        }
        if dash_urls.is_empty() {
            Err(anyhow::anyhow!("no dash url found"))
        } else {
            Ok(dash_urls)
        }
    }

    #[allow(dead_code)]
    async fn decode_m3u8(&self, client: &Client, url: &str) -> anyhow::Result<String> {
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

    pub async fn get_live(&self, room_url: &str) -> anyhow::Result<HashMap<String, String>> {
        let client = reqwest::Client::builder()
            .user_agent(utils::gen_ua())
            .timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let url = url::Url::parse(room_url)?;
        let room_url = if url.as_str().contains("youtube.com/@") {
            let cid = url
                .path_segments()
                .ok_or_else(|| dmlerr!())?
                .last()
                .ok_or_else(|| dmlerr!())?
                .strip_prefix("@")
                .ok_or_else(|| dmlerr!())?;
            format!("https://www.youtube.com/@{}/live", &cid)
        } else {
            // for q in url.query_pairs() {
            //     if q.0.eq("v") {}
            // }
            let vid = url.query_pairs().find(|q| q.0.eq("v")).unwrap().1;
            format!("https://www.youtube.com/watch?v={}", &vid)
        };
        let resp = client
            .get(&room_url)
            .header("Accept-Language", "en-US")
            .header("Referer", "https://www.youtube.com/")
            .send()
            .await?
            .text()
            .await?;
        let re = Regex::new(r"ytInitialPlayerResponse\s*=\s*(\{.+?\});.*?</script>").unwrap();
        let j: serde_json::Value = serde_json::from_str(&re.captures(&resp).ok_or_else(|| dmlerr!())?[1])?;
        let title = j.pointer("/videoDetails/title").ok_or_else(|| dmlerr!())?.as_str().unwrap().to_string();
        if !(j.pointer("/videoDetails/isLive").ok_or_else(|| dmlerr!())?.as_bool().unwrap()) {
            return Err(anyhow::anyhow!("not on air!"));
        }
        let mpd_url = j.pointer("/streamingData/dashManifestUrl").ok_or_else(|| dmlerr!())?.as_str().unwrap();
        // let hls_url = j.pointer("/streamingData/hlsManifestUrl").ok_or_else(|| dmlerr!())?.as_str().unwrap();

        // let urls = self.decode_m3u8(&client, &hls_url).await?;
        let urls = self.decode_mpd(&client, &mpd_url).await?;

        let mut ret = HashMap::new();
        ret.insert(String::from("url"), urls);
        ret.insert(String::from("title"), title);
        Ok(ret)
    }
}
