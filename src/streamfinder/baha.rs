use crate::dmlerr;
use crate::dmlive::DMLContext;
use anyhow::Result;
use std::rc::Rc;

const BAHA_API1: &'static str = "https://api.gamer.com.tw/anime/v1/video.php";

pub struct Baha {
    ctx: Rc<DMLContext>,
}

impl Baha {
    pub fn new(ctx: Rc<DMLContext>) -> Self {
        Baha { ctx }
    }

    pub async fn get_video(&self) -> Result<()> {
        let client = reqwest::Client::builder()
            .user_agent(crate::utils::gen_ua_safari())
            .connect_timeout(tokio::time::Duration::from_secs(10))
            .build()?;
        let params1 = vec![("videoSn", self.ctx.cm.room_id.as_str())];
        let j = client.get(format!("{}", BAHA_API1)).query(&params1).send().await?.json::<serde_json::Value>().await?;
        let title = j.pointer("/data/anime/title").ok_or_else(|| dmlerr!())?.as_str().unwrap();
        let episodes = j.pointer("/data/anime/episodes/0").ok_or_else(|| dmlerr!())?.as_array().unwrap();
        let mut page = self.ctx.cm.bvideo_info.borrow().current_page;
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

        self.ctx.cm.bvideo_info.borrow_mut().last_page = episodes.len();
        self.ctx.cm.bvideo_info.borrow_mut().current_cid = sn;
        let mut si = self.ctx.cm.stream_info.borrow_mut();
        si.insert(
            "title",
            format!("{}[{page}]", title.get(0..len).ok_or_else(|| dmlerr!())?),
        );
        // no video support
        si.insert("url", "https://127.0.0.1".to_string());
        Ok(())
    }
}
