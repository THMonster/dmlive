pub struct CmdParser {
    pub restart: bool,
}

impl CmdParser {
    pub fn new(s: &str) -> Self {
        let mut restart = false;
        if s.starts_with("qlp:") {
            let s = &s[4..];
            let cmds: Vec<&str> = s.split(',').collect();
            for cmd in cmds.iter() {
                if cmd.trim().eq("r") || cmd.trim().eq("restart") {
                    restart = true;
                }
            }
        }
        Self { restart }
    }
}
