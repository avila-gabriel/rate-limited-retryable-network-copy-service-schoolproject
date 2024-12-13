use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::thread::{self, sleep};
use std::time::Duration;
use std::fs::{File, remove_file, read_to_string};
use std::path::{Path, PathBuf};

#[test]
fn test_put_and_get() {
    let cwd = std::env::current_dir().expect("Failed to get current directory");
    println!("Running test in directory: {}", cwd.display());

    #[cfg(unix)]
    const LOG_FILE_PATH: &str = "/tmp/remcp-serv_daemon.log";

    #[cfg(not(unix))]
    const LOG_FILE_PATH: &str = "daemon.log"; // Placeholder, not used on Windows

    #[cfg(unix)]
    {
        // Start server in daemon mode
        let mut server = Command::new("../target/debug/remcp-serv")
            .arg("--debug")
            .spawn()
            .expect("Failed to start server in daemon mode");

        // Wait for server to daemonize
        sleep(Duration::from_secs(2));

        run_put_and_get_test_logic(&cwd);

        // Optional: Check logs in /tmp/remcp-serv_daemon.log if needed
        // let server_logs = read_to_string(LOG_FILE_PATH).expect("Failed to read daemon log file");
        // println!("Server logs:\n{}", server_logs);

        server.kill().ok();
        server.wait().ok();
    }

    #[cfg(not(unix))]
    {
        // Start server and capture stdout/stderr on non-Unix systems
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

        sleep(Duration::from_secs(2));

        run_put_and_get_test_logic(&cwd);

        server.kill().ok();
        server.wait().ok();
    }
}

fn run_put_and_get_test_logic(cwd: &Path) {
    let test_file_path = "test_upload.txt";
    {
        let mut f = File::create(test_file_path).expect("Failed to create test file");
        writeln!(f, "This is a test file for upload").expect("Failed to write test file");
    }

    let remote_file_path = "storage/test_upload_remote.txt";

    // PUT operation
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

    // Validate remote file exists
    let absolute_remote_file_path: PathBuf = if cfg!(unix) {
        cwd.join(remote_file_path)
    } else {
        cwd.join(remote_file_path.replace("/", "\\"))
    };
    assert!(
        absolute_remote_file_path.exists(),
        "Uploaded file does not exist on the server: {}",
        absolute_remote_file_path.display()
    );

    // GET operation
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

    // Validate content
    let original_content = read_to_string(test_file_path).expect("Failed to read original file");
    let downloaded_content = read_to_string(downloaded_file).expect("Failed to read downloaded file");

    println!("Original file content: {}", original_content.trim());
    println!("Downloaded file content: {}", downloaded_content.trim());
    assert_eq!(original_content, downloaded_content, "Content mismatch after GET");

    // Validate downloaded file exists
    let absolute_downloaded_file_path: PathBuf = cwd.join(downloaded_file);
    assert!(
        absolute_downloaded_file_path.exists(),
        "Downloaded file does not exist locally: {}",
        absolute_downloaded_file_path.display()
    );

    // Cleanup
    remove_file(test_file_path).ok();
    remove_file(downloaded_file).ok();
    remove_file(&absolute_remote_file_path).ok();
}