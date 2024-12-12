// from SocialSisterYi/bilibili-API-collect
use reqwest::header::USER_AGENT;
use serde::Deserialize;
use std::time::{SystemTime, UNIX_EPOCH};

const MIXIN_KEY_ENC_TAB: [usize; 64] = [
    46, 47, 18, 2, 53, 8, 23, 32, 15, 50, 10, 31, 58, 3, 45, 35, 27, 43, 5, 49, 33, 9, 42, 19, 29, 28, 14, 39, 12, 38,
    41, 13, 37, 48, 7, 16, 24, 55, 40, 61, 26, 17, 0, 1, 60, 51, 30, 4, 22, 25, 54, 21, 56, 59, 6, 63, 57, 62, 11, 36,
    20, 34, 44, 52,
];

#[derive(Deserialize)]
struct WbiImg {
    img_url: String,
    sub_url: String,
}

#[derive(Deserialize)]
struct Data {
    wbi_img: WbiImg,
}

#[derive(Deserialize)]
struct ResWbi {
    data: Data,
}

// 对 imgKey 和 subKey 进行字符顺序打乱编码
fn get_mixin_key(orig: &[u8]) -> String {
    MIXIN_KEY_ENC_TAB.iter().take(32).map(|&i| orig[i] as char).collect::<String>()
}

fn get_url_encoded(s: &str) -> String {
    s.chars()
        .filter_map(|c| match c.is_ascii_alphanumeric() || "-_.~".contains(c) {
            true => Some(c.to_string()),
            false => {
                // 过滤 value 中的 "!'()*" 字符
                if "!'()*".contains(c) {
                    return None;
                }
                let encoded =
                    c.encode_utf8(&mut [0; 4]).bytes().fold("".to_string(), |acc, b| acc + &format!("%{:02X}", b));
                Some(encoded)
            }
        })
        .collect::<String>()
}

// 为请求参数进行 wbi 签名
pub fn encode_wbi(params: Vec<(&str, String)>, (img_key, sub_key): (String, String)) -> String {
    let cur_time = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(t) => t.as_secs(),
        Err(_) => panic!("SystemTime before UNIX EPOCH!"),
    };
    _encode_wbi(params, (img_key, sub_key), cur_time)
}

fn _encode_wbi(mut params: Vec<(&str, String)>, (img_key, sub_key): (String, String), timestamp: u64) -> String {
    let mixin_key = get_mixin_key((img_key + &sub_key).as_bytes());
    // 添加当前时间戳
    params.push(("wts", timestamp.to_string()));
    // 重新排序
    params.sort_by(|a, b| a.0.cmp(b.0));
    // 拼接参数
    let query = params
        .iter()
        .map(|(k, v)| format!("{}={}", get_url_encoded(k), get_url_encoded(v)))
        .collect::<Vec<_>>()
        .join("&");
    // 计算签名
    let web_sign = format!("{:?}", md5::compute(query.clone() + &mixin_key));
    // 返回最终的 query
    query + &format!("&w_rid={}", web_sign)
}

pub async fn get_wbi_keys(cookies: &str) -> Result<(String, String), reqwest::Error> {
    let client = reqwest::Client::new();
    let ResWbi { data: Data { wbi_img } } = client
        .get("https://api.bilibili.com/x/web-interface/nav")
        .header(USER_AGENT, crate::utils::gen_ua_safari())
        .header("Referer", "https://www.bilibili.com/")
        .header("Cookie", cookies)
        .send()
        .await?
        .json::<ResWbi>()
        .await?;
    Ok((
        take_filename(wbi_img.img_url).unwrap(),
        take_filename(wbi_img.sub_url).unwrap(),
    ))
}

fn take_filename(url: String) -> Option<String> {
    url.rsplit_once('/').and_then(|(_, s)| s.rsplit_once('.')).map(|(s, _)| s.to_string())
}
