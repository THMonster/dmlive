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
        let client = reqwest::Client::builder().user_agent(utils::gen_ua()).timeout(tokio::time::Duration::from_secs(10)).build()?;
        let mut cid = "".to_owned();
        let mut vurl = if room_url.contains("youtube.com/channel/") {
            let re = Regex::new(r"youtube.com/channel/([^/?]+)").unwrap();
            cid.push_str(re.captures(room_url).ok_or("get_live err 1")?[1].to_string().as_str());
            format!("https://www.youtube.com/channel/{}/live", &cid)
        } else {
            room_url.to_string()
        };
        let mut ret = HashMap::new();
        let mut dash_urls = "".to_owned();
        for _ in 0u8..=1u8 {
            let resp =
                client.get(&vurl).header("Accept-Language", "en-US").header("Referer", "https://www.youtube.com/").send().await?.text().await?;
            let c = || -> Result<serde_json::Value, Box<dyn std::error::Error>> {
                let re = Regex::new(r"ytInitialPlayerResponse\s*=\s*(\{.+?\});.*?</script>").unwrap();
                let j: serde_json::Value = serde_json::from_str(re.captures(&resp).ok_or("get_live err 4")?[1].to_string().as_ref())?;
                if !(j.pointer("/videoDetails/isLive").ok_or("get_live err 7")?.as_bool().ok_or("get_live err 7-2")?) {
                    return Err(Box::new(std::io::Error::new(
                        std::io::ErrorKind::Other,
                        "no stream",
                    )));
                }
                Ok(j)
            };
            let j = if let Ok(it) = c() {
                it
            } else {
                if cid.is_empty() {
                    break;
                }
                let ch_url = format!("https://www.youtube.com/channel/{}/videos", &cid);
                let resp =
                    client.get(&ch_url).header("Accept-Language", "en-US").header("Referer", "https://www.youtube.com/").send().await?.text().await?;
                let re = fancy_regex::Regex::new(r#""gridVideoRenderer"((.(?!"gridVideoRenderer"))(?!"style":"UPCOMING"))+"label":"(LIVE|LIVE NOW|PREMIERING NOW)"([\s\S](?!"style":"UPCOMING"))+?("gridVideoRenderer"|</script>)"#).unwrap();
                let t = re.captures(&resp)?.ok_or("get_live err 2")?.get(0).ok_or("get_live err 2 2")?.as_str();
                let re = Regex::new(r#""gridVideoRenderer".+?"videoId":"(.+?)""#).unwrap();
                let vid = re.captures(t).ok_or("get_live err 3")?[1].to_string();
                vurl.clear();
                vurl.push_str(format!("https://www.youtube.com/watch?v={}", &vid).as_str());
                continue;
            };
            dash_urls.clear();
            let mut video_base_url = Vec::new();
            let mut audio_base_url = None;
            let mut sq = 0;
            let mpd_url = j.pointer("/streamingData/dashManifestUrl").ok_or("get_live err 5")?.as_str().ok_or("get_live err 5-2")?;
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
                .filter(|n| n.tag_name().name() == "AdaptationSet" && n.attribute("mimeType").unwrap_or("").contains("video"))
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
            ret.insert(
                String::from("title"),
                j.pointer("/videoDetails/title").ok_or("get_live err 8")?.as_str().ok_or("get_live err 8-2")?.to_owned(),
            );
            break;
        }
        if dash_urls.is_empty() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::Other,
                "no stream",
            )));
        }
        ret.insert(String::from("url"), dash_urls);
        Ok(ret)
    }
}
