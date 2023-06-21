use std::ops::Deref;

pub struct CmdParser {
    pub restart: bool,
    pub next: bool,
    pub back: bool,
    pub fsup: bool,
    pub fsdown: bool,
    pub nick: bool,
    pub fps: bool,
    pub fs: Option<f64>,
    pub fa: Option<f64>,
    pub speed: Option<u64>,
    pub page: Option<u64>,
}

impl CmdParser {
    pub fn new(s: &str) -> Self {
        let mut restart = false;
        let mut next = false;
        let mut back = false;
        let mut fsup = false;
        let mut fsdown = false;
        let mut nick = false;
        let mut fps = false;
        let mut fs = None;
        let mut fa = None;
        let mut speed = None;
        let mut page = None;
        if s.starts_with("dml:") {
            let s = &s[4..];
            let cmds: Vec<&str> = s.split(',').collect();
            for cmd in cmds.iter() {
                if cmd.trim().eq("r") || cmd.trim().eq("reload") {
                    restart = true;
                } else if cmd.trim().eq("next") {
                    next = true;
                } else if cmd.trim().eq("back") {
                    back = true;
                } else if cmd.trim().eq("fsup") {
                    fsup = true;
                } else if cmd.trim().eq("fsdown") {
                    fsdown = true;
                } else if cmd.trim().eq("nick") {
                    nick = true;
                } else if cmd.trim().eq("fps") {
                    fps = true;
                }
                let subcmds: Vec<&str> = cmd.split('=').collect();
                let mut iter = subcmds.iter();
                let arg1 = iter.next().unwrap_or(&"").deref();
                if arg1.eq("fs") {
                    let arg2 = iter.next().unwrap_or(&"").deref();
                    fs = match arg2.parse::<f64>() {
                        Ok(it) => Some(it),
                        Err(_) => None,
                    };
                } else if arg1.eq("fa") {
                    let arg2 = iter.next().unwrap_or(&"").deref();
                    fa = match arg2.parse::<f64>() {
                        Ok(it) => Some(it),
                        Err(_) => None,
                    };
                } else if arg1.eq("speed") {
                    let arg2 = iter.next().unwrap_or(&"").deref();
                    speed = match arg2.parse::<u64>() {
                        Ok(it) => Some(it),
                        Err(_) => None,
                    };
                } else if arg1.eq("p") || arg1.eq("page") {
                    let arg2 = iter.next().unwrap_or(&"").deref();
                    page = match arg2.parse::<u64>() {
                        Ok(it) => Some(it),
                        Err(_) => None,
                    };
                }
            }
        }
        Self {
            restart,
            next,
            back,
            fsup,
            fsdown,
            nick,
            fps,
            fs,
            fa,
            speed,
            page,
        }
    }
}
