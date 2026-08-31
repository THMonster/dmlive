use anyhow::{Context, Result, anyhow};
use serde::Deserialize;

const PLAYER_INFO_API: &str = "https://api.bilibili.com/x/player/wbi/v2";

#[derive(Clone, Debug, Deserialize)]
struct SubtitleTrack {
    lan: String,
    #[serde(default)]
    lan_doc: String,
    #[serde(default)]
    subtitle_url: String,
    #[serde(default)]
    subtitle_url_v2: String,
    #[serde(rename = "type", default)]
    kind: u8,
}

impl SubtitleTrack {
    fn url(&self) -> &str {
        if self.subtitle_url.is_empty() {
            &self.subtitle_url_v2
        } else {
            &self.subtitle_url
        }
    }

    fn is_ai(&self) -> bool {
        self.kind == 1
    }

    fn is_chinese(&self) -> bool {
        self.lan.to_ascii_lowercase().contains("zh") || self.lan_doc.contains("中文") || self.lan_doc.contains("汉语")
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtitleCue {
    pub from_ms: u64,
    pub to_ms: u64,
    pub content: String,
}

#[derive(Clone, Debug)]
pub struct SelectedSubtitle {
    pub language: String,
    pub title: String,
    pub is_ai: bool,
    pub cues: Vec<SubtitleCue>,
}

#[derive(Deserialize)]
struct ApiResponse {
    code: i64,
    #[serde(default)]
    message: String,
    data: Option<PlayerInfo>,
}

#[derive(Deserialize)]
struct PlayerInfo {
    subtitle: Option<SubtitleInfo>,
}

#[derive(Deserialize)]
struct SubtitleInfo {
    #[serde(default)]
    subtitles: Vec<SubtitleTrack>,
}

#[derive(Deserialize)]
struct SubtitleBody {
    #[serde(default)]
    body: Vec<RawCue>,
}

#[derive(Deserialize)]
struct RawCue {
    from: f64,
    to: f64,
    #[serde(default)]
    content: String,
}

fn select_track(tracks: &[SubtitleTrack]) -> Option<&SubtitleTrack> {
    tracks.iter().filter(|track| !track.url().is_empty()).min_by_key(|track| {
        match (track.is_chinese(), track.is_ai()) {
            (true, false) => 0,
            (true, true) => 1,
            (false, false) => 2,
            (false, true) => 3,
        }
    })
}

pub fn normalize_subtitle_url(value: &str) -> Result<String> {
    let value = if value.starts_with("//") {
        format!("https:{value}")
    } else {
        value.to_string()
    };
    let url = url::Url::parse(&value).context("invalid subtitle URL")?;
    match url.scheme() {
        "http" | "https" => Ok(value),
        _ => Err(anyhow!("unsupported subtitle URL scheme")),
    }
}

fn cue_from_raw(raw: RawCue) -> Option<SubtitleCue> {
    if !raw.from.is_finite() || !raw.to.is_finite() || raw.from < 0.0 || raw.to <= raw.from {
        return None;
    }
    let content = raw.content.trim().to_string();
    if content.is_empty() {
        return None;
    }
    Some(SubtitleCue {
        from_ms: (raw.from * 1000.0).round() as u64,
        to_ms: (raw.to * 1000.0).round() as u64,
        content,
    })
}

pub async fn fetch_selected(bvid: &str, cid: &str, cookies: &str) -> Result<Option<SelectedSubtitle>> {
    let keys = crate::utils::bili_wbi::get_wbi_keys(cookies).await?;
    let query = crate::utils::bili_wbi::encode_wbi(
        vec![("bvid", bvid.to_string()), ("cid", cid.to_string())],
        keys,
    );
    let client = reqwest::Client::builder()
        .user_agent(crate::utils::gen_ua_safari())
        .connect_timeout(tokio::time::Duration::from_secs(10))
        .timeout(tokio::time::Duration::from_secs(20))
        .build()?;
    let response = client
        .get(format!("{PLAYER_INFO_API}?{query}"))
        .header("Referer", "https://www.bilibili.com/")
        .header("Cookie", cookies)
        .send()
        .await?
        .error_for_status()?
        .json::<ApiResponse>()
        .await?;
    if response.code != 0 {
        return Err(anyhow!(
            "player info error {}: {}",
            response.code,
            response.message
        ));
    }
    let tracks = response.data.and_then(|data| data.subtitle).map(|info| info.subtitles).unwrap_or_default();
    let Some(track) = select_track(&tracks) else {
        return Ok(None);
    };
    let subtitle_url = normalize_subtitle_url(track.url())?;
    let body = client
        .get(subtitle_url)
        .header("Referer", "https://www.bilibili.com/")
        .header("Cookie", cookies)
        .send()
        .await?
        .error_for_status()?
        .json::<SubtitleBody>()
        .await?;
    let cues = body.body.into_iter().filter_map(cue_from_raw).collect::<Vec<_>>();
    if cues.is_empty() {
        return Ok(None);
    }
    Ok(Some(SelectedSubtitle {
        language: track.lan.clone(),
        title: if track.lan_doc.trim().is_empty() {
            track.lan.clone()
        } else {
            track.lan_doc.clone()
        },
        is_ai: track.is_ai(),
        cues,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(lan: &str, ai: bool) -> SubtitleTrack {
        SubtitleTrack {
            lan: lan.into(),
            lan_doc: lan.into(),
            subtitle_url: "//example.com/subtitle.json".into(),
            subtitle_url_v2: String::new(),
            kind: u8::from(ai),
        }
    }

    #[test]
    fn subtitle_selection_follows_default_priority() {
        let tracks = vec![track("en", false), track("zh-CN", true), track("zh-TW", false)];
        assert_eq!(select_track(&tracks).unwrap().lan, "zh-TW");
        let tracks = vec![track("en", false), track("zh-CN", true)];
        assert_eq!(select_track(&tracks).unwrap().lan, "zh-CN");
        let tracks = vec![track("en", true), track("ja", false)];
        assert_eq!(select_track(&tracks).unwrap().lan, "ja");
        assert!(select_track(&[]).is_none());
    }

    #[test]
    fn subtitle_urls_are_normalized_and_validated() {
        assert_eq!(
            normalize_subtitle_url("//example.com/a").unwrap(),
            "https://example.com/a"
        );
        assert_eq!(
            normalize_subtitle_url("https://example.com/a").unwrap(),
            "https://example.com/a"
        );
        assert!(normalize_subtitle_url("file:///tmp/a").is_err());
    }

    #[test]
    fn invalid_cues_are_discarded() {
        let cue = cue_from_raw(RawCue {
            from: 1.234,
            to: 2.345,
            content: " test ".into(),
        })
        .unwrap();
        assert_eq!(
            cue,
            SubtitleCue {
                from_ms: 1234,
                to_ms: 2345,
                content: "test".into()
            }
        );
        assert!(
            cue_from_raw(RawCue {
                from: -1.0,
                to: 2.0,
                content: "x".into()
            })
            .is_none()
        );
        assert!(
            cue_from_raw(RawCue {
                from: 2.0,
                to: 2.0,
                content: "x".into()
            })
            .is_none()
        );
        assert!(
            cue_from_raw(RawCue {
                from: f64::NAN,
                to: 2.0,
                content: "x".into()
            })
            .is_none()
        );
    }
}
