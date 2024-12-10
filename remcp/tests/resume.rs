use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::thread::{self, sleep};
use std::time::Duration;
use std::fs::{File, remove_file};
use std::path::Path;

#[test]
fn test_resume_put() {
    let cwd = std::env::current_dir().expect("Failed to get current directory");
    println!("Running resume test in directory: {}", cwd.display());

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

    let test_file_path = "test_large_upload.txt";
    {
        let mut f = File::create(test_file_path).expect("Failed to create test file");
        for i in 0..1024 {
            writeln!(f, "This is line number: {}", i).expect("Failed to write test file");
        }
    }

    let remote_file_path = "test_large_upload_remote.txt";

    let mut client = Command::new("../target/debug/remcp")
        .arg(test_file_path)
        .arg(format!("127.0.0.1:{}", remote_file_path))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to run client (PUT)");

    if let Some(stdout) = client.stdout.take() {
        let stdout_reader = BufReader::new(stdout);
        thread::spawn(move || {
            for line in stdout_reader.lines() {
                if let Ok(line) = line {
                    println!("[CLIENT STDOUT] {}", line);
                }
            }
        });
    }

    if let Some(stderr) = client.stderr.take() {
        let stderr_reader = BufReader::new(stderr);
        thread::spawn(move || {
            for line in stderr_reader.lines() {
                if let Ok(line) = line {
                    eprintln!("[CLIENT STDERR] {}", line);
                }
            }
        });
    }

    thread::sleep(Duration::from_millis(10));
    client.kill().expect("Failed to kill client mid-transfer");
    let _ = client.wait().ok();

    let mut client_resume = Command::new("../target/debug/remcp")
        .arg(test_file_path)
        .arg(format!("127.0.0.1:{}", remote_file_path))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to run client (PUT) resume");

    if let Some(stdout) = client_resume.stdout.take() {
        let stdout_reader = BufReader::new(stdout);
        thread::spawn(move || {
            for line in stdout_reader.lines() {
                if let Ok(line) = line {
                    println!("[CLIENT_RESUME STDOUT] {}", line);
                }
            }
        });
    }

    if let Some(stderr) = client_resume.stderr.take() {
        let stderr_reader = BufReader::new(stderr);
        thread::spawn(move || {
            for line in stderr_reader.lines() {
                if let Ok(line) = line {
                    eprintln!("[CLIENT_RESUME STDERR] {}", line);
                }
            }
        });
    }

    let status = client_resume.wait().expect("Failed to wait on resumed client");
    assert!(status.success(), "Client (PUT resume) failed");
    println!("PUT resume operation completed successfully.");

    let absolute_remote_file_path = format!("{}\\{}", cwd.display(), remote_file_path);
    assert!(
        Path::new(&absolute_remote_file_path).exists(),
        "Uploaded file does not exist on the server: {}",
        absolute_remote_file_path
    );

    sleep(Duration::from_secs(1));
    let original_content = std::fs::read_to_string(test_file_path).expect("Failed to read original file");
    let uploaded_content = std::fs::read_to_string(&absolute_remote_file_path).expect("Failed to read uploaded file");

    assert_eq!(original_content, uploaded_content, "Content mismatch after resume PUT");

    println!("Resume PUT test passed successfully.");

    // Cleanup
    remove_file(test_file_path).ok();
    remove_file(&absolute_remote_file_path).ok();

    server.kill().ok();
    server.wait().ok();
}