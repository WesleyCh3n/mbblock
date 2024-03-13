use std::process::{Child, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Parser;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Parser, Debug)]
#[clap(author, version)]
struct Args {
    #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
    device: Vec<String>,
    #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
    str: Vec<String>,
    #[clap(short, long, default_value_t = 60*5)]
    timeout: u64,
}

#[tokio::main]
async fn main() {
    let args = Args::parse();

    std::process::Command::new("dmesg")
        .arg("-C")
        .output()
        .unwrap();
    let mut child = Command::new("dmesg")
        .arg("--follow")
        .stdin(Stdio::inherit())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().expect("Failed to get stdout");
    let mut reader = BufReader::new(stdout).lines();

    let stop = Arc::new(AtomicBool::new(false));
    let stop_clone = stop.clone();

    tokio::spawn(async move {
        'outer: while let Some(line) =
            reader.next_line().await.unwrap_or_default()
        {
            println!("[INFO]: {}", line);
            for s in &args.str {
                if line.contains(s) {
                    println!("\n{:=^80}", "");
                    println!("[FOUND ERROR]: {}", line);
                    println!("{:=^80}\n", "");
                    stop_clone.store(true, Ordering::Relaxed);
                    break 'outer;
                }
            }
        }
    });

    let mut childs = args
        .device
        .into_iter()
        .map(|device| {
            std::process::Command::new("badblocks")
                .args(
                    format!("-s -v {}", device)
                        .split(' ')
                        .collect::<Vec<&str>>(),
                )
                .spawn()
                .unwrap()
        })
        .collect::<Vec<Child>>();

    let start = std::time::SystemTime::now();
    loop {
        if start.elapsed().unwrap().as_secs() > args.timeout {
            println!("\nTime exceeded");
            break;
        }
        if stop.load(Ordering::Relaxed) {
            println!("\ndmesg found stop string");
            break;
        }
        let mut is_all_finish = true;
        for child in childs.iter_mut() {
            is_all_finish = child.try_wait().ok().and_then(|r| r).is_none(); // still running
        }
        if is_all_finish {
            println!("All badblocks finish");
            break;
        }
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    for child in &mut childs {
        child.kill().unwrap();
    }
    child.kill().await.unwrap();
}
