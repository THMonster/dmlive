use crate::config::ConfigManager;
use crate::dmlerr;
use anyhow::Result;
use std::collections::HashMap;
use std::rc::Rc;
use url::Url;

const BAHA_API1: &'static str = "https://api.gamer.com.tw/anime/v1/video.php";

pub struct Baha {
    cm: Rc<ConfigManager>,
}

impl Baha {
    pub fn new(cm: Rc<ConfigManager>) -> Self {
        Baha { cm }
    }

    pub async fn get_video(&self) -> Result<HashMap<&'static str, String>> {
        let mut sn = "".to_string();
        let u = Url::parse(&self.cm.room_url).unwrap();
        for q in u.query_pairs() {
            if q.0.eq("sn") {
                sn = q.1.parse().unwrap();
            }
        }
        let mut ret = HashMap::new();
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua_safari())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let params1 = vec![("videoSn", sn.as_str())];
        let j = client.get(format!("{}", BAHA_API1)).query(&params1).send().await?.json::<serde_json::Value>().await?;
        let title = j.pointer("/data/anime/title").ok_or_else(|| dmlerr!())?.as_str().unwrap();
        let episodes = j.pointer("/data/anime/episodes/0").ok_or_else(|| dmlerr!())?.as_array().unwrap();
        let mut page = self.cm.bvideo_info.borrow().current_page;
        if page == 0 {
            page = 1;
        }
        let ep = match episodes.get(page - 1) {
            Some(ep) => ep,
            None => {
                page = episodes.len();
                episodes.last().ok_or_else(|| dmlerr!())?
            }
        };
        let sn = ep.pointer("/videoSn").ok_or_else(|| dmlerr!())?.as_u64().unwrap().to_string();
        let len = title.len() - 3;
        ret.insert(
            "title",
            format!("{}[{}]", title.get(0..len).ok_or_else(|| dmlerr!())?, page),
        );
        ret.insert("bili_cid", sn);
        // no video support
        ret.insert("url", "https://127.0.0.1".to_string());
        Ok(ret)
    }
}
