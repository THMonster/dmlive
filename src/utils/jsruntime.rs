use std::cell::RefCell;

use anyhow::Result;
use log::info;
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader, Lines},
    process::{Child, ChildStdin, ChildStdout, Command},
};

const JS_WORKER: &'static str = r#"const vm=require("vm"),ctx=vm.createContext({});process.stdin.on("data",(t=>{try{const e=vm.runInContext(t.toString(),ctx);process.stdout.write(e+"\n")}catch(t){process.stdout.write(t.message+"\n")}}));"#;

pub struct JSRuntime {
    rt: RefCell<Option<Child>>,
    rtin: RefCell<Option<ChildStdin>>,
    rtout: RefCell<Option<Lines<BufReader<ChildStdout>>>>,
}
impl JSRuntime {
    pub fn new() -> Self {
        Self {
            rt: RefCell::new(None),
            rtin: RefCell::new(None),
            rtout: RefCell::new(None),
        }
    }

    pub async fn eval(&self, js: &str) -> Result<String> {
        self.rtin.borrow_mut().as_mut().unwrap().write_all(js.as_bytes()).await?;
        let mut reader = self.rtout.borrow_mut();
        let ret = reader.as_mut().unwrap().next_line().await?.unwrap_or("".to_string());
        info!("{}", &ret);
        Ok(ret)
    }

    async fn run_deno(&self) -> Result<Child> {
        let rt = Command::new("deno")
            .arg("eval")
            .arg(include_str!("worker_deno.js"))
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        Ok(rt)
    }

    async fn run_node(&self) -> Result<Child> {
        let rt = Command::new("node")
            .arg("-e")
            .arg(JS_WORKER)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .kill_on_drop(true)
            .spawn()?;
        Ok(rt)
    }

    pub async fn run(&self) -> Result<()> {
        let mut rt = if let Ok(rt) = self.run_deno().await {
            info!("using deno");
            rt
        } else {
            info!("using node");
            self.run_node().await?
        };
        let reader = BufReader::new(rt.stdout.take().unwrap()).lines();
        *self.rtin.borrow_mut() = Some(rt.stdin.take().unwrap());
        *self.rtout.borrow_mut() = Some(reader);
        *self.rt.borrow_mut() = Some(rt);
        Ok(())
    }
}
