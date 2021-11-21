use std::{
    collections::{HashMap, LinkedList},
    sync::{Arc, Mutex},
};

use futures::{stream::StreamExt, SinkExt};
use regex::Regex;
use reqwest::Url;
use tokio::time::sleep;
use tokio_tungstenite::connect_async;

pub struct Twitch {
    heartbeat: String,
}

impl Twitch {
    pub fn new() -> Self {
        Twitch {
            heartbeat: "PING".to_string(),
        }
    }

    async fn get_ws_info(
        &self,
        url: &str,
    ) -> Result<(String, Vec<String>), Box<dyn std::error::Error>> {
        let rid = Url::parse(url)?
            .path_segments()
            .ok_or("rid parse error 1")?
            .last()
            .ok_or("rid parse error 2")?
            .to_string();
        let mut reg_datas: Vec<String> = Vec::new();

        reg_datas
            .push("CAP REQ :twitch.tv/tags twitch.tv/commands twitch.tv/membership".to_owned());
        reg_datas.push("PASS SCHMOOPIIE".to_owned());
        let rn = rand::random::<u64>();
        let nick = format!("justinfan{}", 10000 + (rn % 80000));
        reg_datas.push(format!("NICK {}", &nick));
        reg_datas.push(format!("USER {0} 8 * :{0}", &nick));
        reg_datas.push(format!("JOIN #{}", &rid));
        // println!("{:?}", &reg_datas);
        Ok(("wss://irc-ws.chat.twitch.tv".to_string(), reg_datas))
    }

    fn decode_msg(
        &self,
        data: &mut Vec<u8>,
    ) -> Result<Vec<HashMap<String, String>>, Box<dyn std::error::Error>> {
        let mut ret = Vec::new();
        let msg = String::from_utf8_lossy(data);
        for m in msg.split('\n') {
            let mut d = std::collections::HashMap::new();
            let re = Regex::new(r#"display-name=([^;]+);"#).unwrap();
            let name = match re.captures(&m) {
                Some(it) => it[1].to_string(),
                _ => continue,
            };
            let re = Regex::new(r#"PRIVMSG [^:]+:(.+)"#).unwrap();
            let content = match re.captures(&m) {
                Some(it) => it[1].to_string(),
                _ => continue,
            };
            let re = Regex::new(r#"color=#([a-zA-Z0-9]{6});"#).unwrap();
            let color = match re.captures(&m) {
                Some(it) => it[1].to_string(),
                None => "ffffff".to_owned(),
            };
            d.insert("msg_type".to_owned(), "danmaku".to_owned());
            d.insert("name".to_owned(), name);
            d.insert("content".to_owned(), content);
            d.insert("color".to_owned(), color);
            ret.push(d);
        }
        Ok(ret)
    }

    pub async fn run(
        &self,
        url: &str,
        dtx: async_channel::Sender<(String, String, String)>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let (ws, mut reg_datas) = self.get_ws_info(url).await?;
        let (ws_stream, _) = connect_async(&ws).await?;
        let (mut ws_write, mut ws_read) = ws_stream.split();
        for reg_data in reg_datas.drain(..) {
            ws_write
                .send(tokio_tungstenite::tungstenite::Message::text(reg_data))
                .await?;
        }
        let hb = self.heartbeat.clone();
        tokio::spawn(async move {
            loop {
                sleep(tokio::time::Duration::from_secs(20)).await;
                let hb1 = hb.clone();
                match ws_write
                    .send(tokio_tungstenite::tungstenite::Message::text(hb1))
                    .await
                {
                    Ok(_) => {}
                    _ => {
                        println!("send heartbeat failed!")
                    }
                };
            }
        });
        while let Some(m) = ws_read.next().await {
            match m {
                Ok(it) => {
                    let mut dm = self.decode_msg(it.into_data().as_mut())?;
                    for d in dm.drain(..) {
                        dtx.send((
                            d.get("color").unwrap_or(&"ffffff".into()).into(),
                            d.get("name").unwrap_or(&"unknown".into()).into(),
                            d.get("content").unwrap_or(&" ".into()).into(),
                        ))
                        .await?;
                    }
                }
                Err(e) => {
                    println!("read ws error: {:?}", e);
                    break;
                }
            }
        }
        println!("ws closed!");
        Ok(())
    }
}
