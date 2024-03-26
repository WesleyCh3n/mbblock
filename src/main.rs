use chrono::offset::Local;
use chrono::DateTime;
use std::io::Write;
use std::process::{ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use clap::Parser;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::{Child, Command};

#[derive(Parser, Debug)]
#[clap(author, version)]
struct Args {
    #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
    device: Vec<String>,
    #[clap(short, long, value_parser, num_args = 1.., value_delimiter = ' ')]
    str: Vec<String>,
    #[clap(short, long)]
    timeout: Option<u64>,
}

#[tokio::main]
async fn main() -> ExitCode {
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
            if line.contains("ata") {
                println!("[INFO]: {}", line);
            }
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
            println!("start badblocks -s -v {}", device);
            Command::new("badblocks")
                .args(
                    format!("-s -v {}", device)
                        .split(' ')
                        .collect::<Vec<&str>>(),
                )
                .stdout(Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<Child>>();

    let start = std::time::SystemTime::now();
    let mut is_failed = false;
    let mut success = true;
    loop {
        std::thread::sleep(std::time::Duration::from_millis(100));
        if let Some(timeout) = args.timeout {
            if start.elapsed().unwrap().as_secs() > timeout {
                println!("\nTime exceeded");
                break;
            }
        }
        if stop.load(Ordering::Relaxed) {
            println!("\ndmesg found stop string");
            is_failed = true;
            break;
        }
        let mut running = false;
        success = true;
        for child in childs.iter_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    if !status.success() {
                        success = false;
                        break;
                    }
                }
                Ok(None) => {
                    running = true;
                }
                Err(e) => println!("error attempting to wait: {e}"),
            }
        }
        if !success {
            println!("\nbadblocks failed");
            break;
        }
        if !running {
            println!("\nAll badblocks finish");
            break;
        }

        let local_time: DateTime<Local> = std::time::SystemTime::now().into();
        print!("Runing {}\r", local_time.format("%Y/%m/%d %T"));
        std::io::stdout().flush().unwrap();
    }
    let end = std::time::SystemTime::now();
    let elapsed = end.duration_since(start).unwrap();
    for child in &mut childs {
        child.kill().await.unwrap_or_default();
    }
    child.kill().await.unwrap_or_default();
    if !success {
        return ExitCode::FAILURE;
    }

    let start_local: DateTime<Local> = start.into();
    let end_local: DateTime<Local> = end.into();

    println!("\n{:=^80}", "Result");
    println!("Start: {}", start_local.format("%Y/%m/%d %T"));
    println!("End: {}", end_local.format("%Y/%m/%d %T"));
    println!("Elapsed: {:?}", elapsed);

    if is_failed {
        println!("badblocks failed");
    } else {
        println!("badblocks success");
    }

    if is_failed {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
