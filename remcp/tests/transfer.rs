use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::thread;
use std::time::Duration;
use std::fs::{File, remove_file};
use std::path::Path;

#[test]
fn test_put_and_get() {
    let cwd = std::env::current_dir().expect("Failed to get current directory");
    println!("Running test in directory: {}", cwd.display());

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

    let test_file_path = "test_upload.txt";
    {
        let mut f = File::create(test_file_path).expect("Failed to create test file");
        writeln!(f, "This is a test file for upload").expect("Failed to write test file");
    }

    let remote_file_path = r"storage\test_upload_remote.txt";

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

    let status = client.wait().expect("Failed to wait on client");
    assert!(status.success(), "Client PUT failed");

    let absolute_remote_file_path = r"C:\Users\gabri\OneDrive\Área de Trabalho\odo\remcp_project\remcp\storage\test_upload_remote.txt";
    assert!(
        Path::new(absolute_remote_file_path).exists(),
        "Uploaded file does not exist on the server: {}",
        absolute_remote_file_path
    );

    let downloaded_file = "test_download.txt";
    if Path::new(downloaded_file).exists() {
        remove_file(downloaded_file).ok();
    }

    let mut client2 = Command::new("../target/debug/remcp")
        .arg(format!("127.0.0.1:{}", remote_file_path))
        .arg(downloaded_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Failed to run client (GET)");

    if let Some(stdout) = client2.stdout.take() {
        let stdout_reader = BufReader::new(stdout);
        thread::spawn(move || {
            for line in stdout_reader.lines() {
                if let Ok(line) = line {
                    println!("[CLIENT2 STDOUT] {}", line);
                }
            }
        });
    }

    if let Some(stderr) = client2.stderr.take() {
        let stderr_reader = BufReader::new(stderr);
        thread::spawn(move || {
            for line in stderr_reader.lines() {
                if let Ok(line) = line {
                    eprintln!("[CLIENT2 STDERR] {}", line);
                }
            }
        });
    }

    let status2 = client2.wait().expect("Failed to wait on client2");
    assert!(status2.success(), "Client GET failed");

    let original_content = std::fs::read_to_string(test_file_path).expect("Failed to read original file");
    let downloaded_content = std::fs::read_to_string(downloaded_file).expect("Failed to read downloaded file");

    println!("Original file content: {}", original_content.trim());
    println!("Downloaded file content: {}", downloaded_content.trim());
    assert_eq!(original_content, downloaded_content, "Content mismatch after GET");

    let absolute_downloaded_file_path = format!("{}\\{}", cwd.display(), downloaded_file);
    assert!(
        Path::new(&absolute_downloaded_file_path).exists(),
        "Downloaded file does not exist locally: {}",
        absolute_downloaded_file_path
    );

    remove_file(test_file_path).ok();
    remove_file(downloaded_file).ok();
    remove_file(remote_file_path).ok();
    server.kill().ok();
    server.wait().ok();
}