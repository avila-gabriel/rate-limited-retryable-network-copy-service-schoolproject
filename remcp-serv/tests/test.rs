use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::thread;
use std::time::Duration;

#[test]
fn test_server_starts() {
    let mut server = Command::new("../target/debug/remcp-serv")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to start server");

    if let Some(stdout) = server.stdout.take() {
        let stdout_reader = BufReader::new(stdout);
        thread::spawn(move || {
            for line in stdout_reader.lines() {
                if let Ok(line) = line {
                    println!("[SERVER STDOUT] {}", line);
                }
            }
        });
    }

    if let Some(stderr) = server.stderr.take() {
        let stderr_reader = BufReader::new(stderr);
        thread::spawn(move || {
            for line in stderr_reader.lines() {
                if let Ok(line) = line {
                    eprintln!("[SERVER STDERR] {}", line);
                }
            }
        });
    }

    thread::sleep(Duration::from_secs(2));
    server.kill().ok();
    server.wait().ok();
}