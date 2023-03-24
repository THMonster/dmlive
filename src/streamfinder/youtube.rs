use log::debug;
use regex::Regex;
use std::collections::HashMap;

use crate::utils;

pub struct Youtube {}

impl Youtube {
    pub fn new() -> Self {
        Youtube {}
    }

    pub async fn get_live(&self, room_url: &str) -> Result<HashMap<String, String>, Box<dyn std::error::Error>> {
        let client = reqwest::Client::builder()
            .user_agent(utils::gen_ua())
            .timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let url = url::Url::parse(room_url)?;
        let room_url = if url.as_str().contains("youtube.com/channel/") {
            let cid = url.path_segments().ok_or("gl err a1")?.last().ok_or("gl err a12")?;
            format!("https://www.youtube.com/channel/{}/live", &cid)
        } else {
            for q in url.query_pairs() {
                if q.0.eq("v") {}
            }
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
        let j: serde_json::Value = serde_json::from_str(&re.captures(&resp).ok_or("gl err b1")?[1])?;
        // let vid = j.pointer("/videoDetails/videoId").ok_or("gl err b2")?.as_str().unwrap().to_string();
        // let cid = j.pointer("/videoDetails/channelId").ok_or("gl err b3")?.as_str().unwrap().to_string();
        let title = j.pointer("/videoDetails/title").ok_or("gl err b4")?.as_str().unwrap().to_string();
        if !(j.pointer("/videoDetails/isLive").ok_or("gl err b5")?.as_bool().unwrap()) {
            return Err("gl err b6".into());
        }

        let mut ret = HashMap::new();
        let mut dash_urls = "".to_owned();
        let mut video_base_url = Vec::new();
        let mut audio_base_url = None;
        let mut sq = 0;
        let mpd_url = j.pointer("/streamingData/dashManifestUrl").ok_or("gl err c1")?.as_str().unwrap();
        let resp = client
            .get(mpd_url)
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
            return Err("gl err d1".into());
        }
        ret.insert(String::from("url"), dash_urls);
        ret.insert(String::from("title"), title);
        Ok(ret)
    }
}
