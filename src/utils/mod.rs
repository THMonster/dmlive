pub mod bili_wbi;
pub mod cookies;
// pub mod jsruntime;

use tokio::process::Command;

#[macro_export]
macro_rules! dmlerr {
    ($($args: expr),*) => {
        anyhow::anyhow!(
            "file: {}, line: {}, column: {}",
            file!(),
            line!(),
            column!()
        )
    };
}

pub fn gen_ua() -> String {
    // let rn = rand::random::<u64>();
    // let n1 = 50 + (rn % 30);
    // let n2 = 1000 + (rn % 6000);
    // let n3 = 10 + (rn % 150);
    // format!(
    //     "Mozilla/5.0 (Windows NT 10.0; WOW64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{}.0.{}.{} Safari/537.36",
    //     n1, n2, n3
    // );
    "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:147.0) Gecko/20100101 Firefox/147.0".into()
    // "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/96.0.4664.45 Safari/537.36".into()
    // "Mozilla/5.0 (X11; Linux x86_64; rv:94.0) Gecko/20100101 Firefox/94.0".into()
    // "Mozilla/5.0 (Android 10; Mobile; rv:94.0) Gecko/94.0 Firefox/94.0".into()
    // "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/96.0.4664.45 Safari/537.36"
    //     .into()
}

pub fn gen_ua_safari() -> String {
    "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/15.4 Safari/605.1.15".into()
}

pub fn vn(mut val: u64) -> Vec<u8> {
    let mut buf = b"".to_vec();
    while (val >> 7) != 0 {
        let m = val & 0xFF | 0x80;
        buf.push(m.to_le_bytes()[0]);
        val >>= 7;
    }
    buf.push(val.to_le_bytes()[0]);
    buf
}

pub fn tp(a: u64, b: u64, ary: &[u8]) -> Vec<u8> {
    let mut v = vn((b << 3) | a);
    v.append(ary.to_vec().as_mut());
    v
}

pub fn rs(a: u64, ary: &[u8]) -> Vec<u8> {
    let mut v = vn(ary.len() as u64);
    v.append(ary.to_vec().as_mut());
    tp(2, a, &v)
}

pub fn nm(a: u64, ary: u64) -> Vec<u8> {
    tp(0, a, &vn(ary))
}

pub fn _str_to_ms(time_str: &str) -> u64 {
    let mut t = time_str.trim().rsplit(':');
    let mut ret = 0f64;
    let mut i = 0usize;
    while let Some(it) = t.next() {
        if i == 0 {
            let s: f64 = it.parse().unwrap_or(0.0);
            ret += s;
        } else if i == 1 {
            let m: f64 = it.parse().unwrap_or(0.0);
            ret += m * 60.0;
        } else if i == 2 {
            let h: f64 = it.parse().unwrap_or(0.0);
            ret += h * 60.0 * 60.0;
        } else {
            break;
        }
        i = i.saturating_add(1);
    }
    (ret * 1000.0) as u64
}

pub async fn is_android() -> bool {
    let output = Command::new("getprop").arg("ro.build.version.release").output();
    output.await.map(|x| x.status.success()).unwrap_or(false)
}
