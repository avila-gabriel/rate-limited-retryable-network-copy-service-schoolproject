use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader};
use std::thread;
use std::time::Duration;

#[test]
fn test_server_starts() {
    #[cfg(unix)]
    {
        // Start server in daemon mode
        let mut server = Command::new("../target/debug/remcp-serv")
            .arg("--debug")
            .spawn()
            .expect("Failed to start server in daemon mode");

        // Wait for server to daemonize
        thread::sleep(Duration::from_secs(2));

        // Check if the daemon has written its logs
        let log_path = "/tmp/remcp-serv_daemon.log";
        let logs = std::fs::read_to_string(log_path).expect("Failed to read daemon log file");
        println!("Server daemon logs:\n{}", logs);

        // Validate the log contains the expected startup message
        assert!(logs.contains("Server running on port 7878"), "Server did not start correctly");

        server.kill().ok();
        server.wait().ok();
    }

    #[cfg(not(unix))]
    {
        // Start server interactively
        let mut server = Command::new("../target/debug/remcp-serv")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start server");

        // Capture stdout and stderr
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

        // Kill and wait for the server process
        server.kill().ok();
        server.wait().ok();
    }
}
