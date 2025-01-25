use std::{fs::DirEntry, time::SystemTime};

use aes::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use log::info;
use sqlx::sqlite::SqlitePoolOptions;
use tokio::process::Command;

#[derive(sqlx::FromRow, Debug)]
struct FirefoxCookie {
    name: String,
    value: String,
}
#[derive(sqlx::FromRow, Debug)]
struct ChromeCookie {
    name: String,
    value: String,
    encrypted_value: Vec<u8>,
}

async fn get_kwallet_password(browser: &str) -> anyhow::Result<[u8; 16]> {
    let browser_keyring_name: String = if browser.eq("chrome") {
        "Chrome".into()
    } else if browser.eq("chromium") {
        "Chromium".into()
    } else {
        return Err(anyhow::anyhow!("unknown browser"));
    };
    let dbus_send_cmd = Command::new("dbus-send")
        .args(&[
            "--session",
            "--print-reply=literal",
            "--dest=org.kde.kwalletd5",
            "/modules/kwalletd5",
            "org.kde.KWallet.networkWallet",
        ])
        .output()
        .await?;
    let wallet_name = String::from_utf8_lossy(&dbus_send_cmd.stdout).trim().to_string();
    info!("found wallet name: {}", &wallet_name);
    let kwallet_cmd = Command::new("kwallet-query")
        .args(&[
            "--read-password",
            format!("{} Safe Storage", &browser_keyring_name).as_str(),
            "--folder",
            format!("{} Keys", &browser_keyring_name).as_str(),
            wallet_name.as_str(),
        ])
        .output()
        .await?;
    let mut password = String::from_utf8_lossy(&kwallet_cmd.stdout).trim().to_string();
    if password.starts_with("Failed") {
        password.clear();
    }
    info!("found password: {}", &password);
    let mut pw_key = [0u8; 16];
    ring::pbkdf2::derive(
        ring::pbkdf2::PBKDF2_HMAC_SHA1,
        std::num::NonZeroU32::new(1).unwrap(),
        b"saltysalt",
        password.as_bytes(),
        &mut pw_key,
    );
    Ok(pw_key)
}

fn decrypt_chrome_cookie(data: &mut [u8], key: &[u8; 16]) -> anyhow::Result<String> {
    if let Some(it) = data.get(0..=2) {
        if it == b"v11" {
            type Aes128CbcDec = cbc::Decryptor<aes::Aes128>;
            let mut buf = data.get(3..).unwrap().to_owned();
            let pt = Aes128CbcDec::new(key.into(), &[32u8; 16].into())
                .decrypt_padded_mut::<Pkcs7>(&mut buf)
                .map_err(|_| anyhow::anyhow!("decryption failed"))?;
            return Ok(String::from_utf8_lossy(pt.get(32..).unwrap_or(b"")).into());
        } else {
            return Err(anyhow::anyhow!("a v10 cookie"));
        }
    }
    todo!()
}

async fn get_chrome_cookies(host: &str, is_chromium: bool) -> anyhow::Result<String> {
    // TODO: detect de
    // let v10_key = b"peanuts";

    let (proj_dirs, v11_key) = if is_chromium {
        (
            directories::ProjectDirs::from("com", "google", "chromium").unwrap(),
            get_kwallet_password("chromium").await?,
        )
    } else {
        (
            directories::ProjectDirs::from("com", "google", "google-chrome").unwrap(),
            get_kwallet_password("chrome").await?,
        )
    };
    let d = proj_dirs.config_dir();
    let cookie_path = d.join("Default/Cookies");
    if !cookie_path.exists() {
        return Err(anyhow::anyhow!("Chrome Cookies file not found!"));
    }

    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", cookie_path.to_string_lossy()))
        .await?;
    let mut cookies = sqlx::query_as::<_, ChromeCookie>(
        format!(
            "
SELECT name, value, encrypted_value 
FROM cookies
WHERE host_key LIKE '{}'
        ",
            host
        )
        .as_str(),
    )
    .fetch_all(&pool) // -> Vec<Country>
    .await?;
    info!("encrypted_cookies: {:?}", &cookies);
    let mut ret: String = "".into();
    for it in cookies.iter_mut() {
        if it.value.is_empty() {
            ret.push_str(
                format!(
                    "{}={};",
                    it.name,
                    decrypt_chrome_cookie(&mut it.encrypted_value, &v11_key)?
                )
                .as_str(),
            );
        } else {
            ret.push_str(format!("{}={};", it.name, it.value).as_str());
        }
    }
    info!("decrypted_cookies: {}", &ret);
    Ok(ret)
}

async fn get_firefox_cookies(host: &str) -> anyhow::Result<String> {
    let user_dirs = directories::UserDirs::new().ok_or_else(|| anyhow::anyhow!("User dir not found"))?;
    let ff_dir = user_dirs.home_dir().join(".mozilla").join("firefox");
    let dir = std::fs::read_dir(ff_dir)?
        .max_by_key(|x| {
            let t = |a: &DirEntry| {
                let ret = a.metadata()?.modified()?;
                anyhow::Ok(ret)
            };
            if let Ok(x) = x {
                info!("{:?}", &x);
                if let Ok(st) = t(x) {
                    st
                } else {
                    SystemTime::UNIX_EPOCH
                }
            } else {
                SystemTime::UNIX_EPOCH
            }
        })
        .ok_or_else(|| anyhow::anyhow!("Firefox dir not found"))??;
    let cookie_path = dir.path().join("cookies.sqlite");
    if !cookie_path.exists() {
        return Err(anyhow::anyhow!("Firefox Cookies file not found!"));
    }
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&format!("sqlite:{}", cookie_path.to_string_lossy()))
        .await?;
    let mut cookies = sqlx::query_as::<_, FirefoxCookie>(
        format!(
            "
SELECT name, value
FROM moz_cookies
WHERE host LIKE '{}'
        ",
            host
        )
        .as_str(),
    )
    .fetch_all(&pool) // -> Vec<Country>
    .await?;
    info!("raw cookies: {:?}", &cookies);
    let mut ret: String = "".into();
    for it in cookies.iter_mut() {
        ret.push_str(format!("{}={};", it.name, it.value).as_str())
    }
    info!("cookies: {}", &ret);
    Ok(ret)
}

pub async fn get_cookies_from_browser(browser: &str, host: &str) -> anyhow::Result<String> {
    if browser.eq("chrome") {
        return get_chrome_cookies(host, false).await;
    } else if browser.eq("firefox") {
        return get_firefox_cookies(host).await;
    } else if browser.eq("chromium") {
        return get_chrome_cookies(host, true).await;
    }
    Err(anyhow::anyhow!("browser not supported"))
}
