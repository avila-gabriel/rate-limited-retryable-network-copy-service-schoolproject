use std::process::{Command, Stdio};
use std::io::{BufRead, BufReader, Write};
use std::thread::{self, sleep};
use std::time::Duration;
use std::fs::{File, remove_file};
use std::path::Path;

#[test]
fn test_bandwidth_distribution() {
    let cwd = std::env::current_dir().expect("Failed to get current directory");
    println!("Running bandwidth distribution test in directory: {}", cwd.display());

    // Configuration depending on platform
    // On Unix: Daemon mode, logs in /tmp/remcp-serv_daemon.log
    // On Windows: Non-daemon mode, capture stdout
    #[cfg(unix)]
    const LOG_FILE_PATH: &str = "/tmp/remcp-serv_daemon.log";
    #[cfg(not(unix))]
    const LOG_FILE_PATH: &str = "daemon.log"; // Unused on non-unix, just a placeholder

    #[cfg(unix)]
    {
        // On Unix, server runs in daemon mode with --debug, logs to file. We don't capture stdout.
        let mut server = Command::new("../target/debug/remcp-serv")
            .arg("--debug")
            .spawn()
            .expect("Failed to start server in daemon mode");

        // Wait for server to daemonize and create the log file
        sleep(Duration::from_secs(2));

        // On Unix, no stdout capture, logs are in LOG_FILE_PATH
        // Just proceed with the test steps (spawn clients, etc.)
        run_test_logic(&cwd, LOG_FILE_PATH);

        // After test logic completes, kill server if needed
        server.kill().ok();
        server.wait().ok();
    }

    #[cfg(not(unix))]
    {
        // On non-unix (e.g., Windows), run the server normally and capture stdout/stderr
        let mut server = Command::new("../target/debug/remcp-serv")
            .arg("--debug")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to start server");

        let server_stdout = server.stdout.take().expect("No server stdout");
        let server_stderr = server.stderr.take().expect("No server stderr");

        let server_stdout_reader = BufReader::new(server_stdout);
        let server_stderr_reader = BufReader::new(server_stderr);

        let mut server_stdout_lines = Vec::new();

        let server_handle = thread::spawn(move || {
            for line in server_stdout_reader.lines() {
                if let Ok(l) = line {
                    println!("[SERVER STDOUT] {}", l);
                    server_stdout_lines.push(l);
                }
            }
            server_stdout_lines
        });

        thread::spawn(move || {
            for line in server_stderr_reader.lines() {
                if let Ok(line) = line {
                    eprintln!("[SERVER STDERR] {}", line);
                }
            }
        });

        sleep(Duration::from_secs(2));

        // Run the main test logic (spawn clients, etc.)
        run_test_logic(&cwd, LOG_FILE_PATH);

        // Kill server and collect stdout
        server.kill().ok();
        let _ = server.wait().ok();
        let server_lines = server_handle.join().expect("Failed to join server handle");

        verify_chunks(server_lines, 5, 256);

        println!("Bandwidth distribution test passed successfully.");
    }
}

// This function contains the main logic for spawning clients and waiting for them.
// It does not handle platform differences by itself; it assumes they're done before/after this call.
fn run_test_logic(cwd: &std::path::Path, _log_file_path: &str) {
    let file_count = 5;
    let transfer_rate = 256;

    // Create files
    let file_names: Vec<String> = (0..file_count).map(|i| format!("test_upload_{}.txt", i)).collect();
    for f_name in &file_names {
        let mut f = File::create(f_name).expect("Failed to create test file");
        for i in 0..64 {
            writeln!(f, "Line {}", i).expect("Failed to write test file");
        }
    }

    let remote_file_names: Vec<String> = (0..file_count).map(|i| format!("test_upload_remote_{}.txt", i)).collect();

    // Spawn multiple PUT clients simultaneously
    let mut client_processes = Vec::new();
    for (i, f_name) in file_names.iter().enumerate() {
        let remote_path = &remote_file_names[i];
        let mut client = Command::new("../target/debug/remcp")
            .arg(f_name)
            .arg(format!("127.0.0.1:{}", remote_path))
            .arg("--debug")
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to run client (PUT)");

        if let Some(stdout) = client.stdout.take() {
            let stdout_reader = BufReader::new(stdout);
            let cid = i;
            thread::spawn(move || {
                for line in stdout_reader.lines() {
                    if let Ok(line) = line {
                        println!("[CLIENT {} STDOUT] {}", cid, line);
                    }
                }
            });
        }

        if let Some(stderr) = client.stderr.take() {
            let cid = i;
            let stderr_reader = BufReader::new(stderr);
            thread::spawn(move || {
                for line in stderr_reader.lines() {
                    if let Ok(line) = line {
                        eprintln!("[CLIENT {} STDERR] {}", cid, line);
                    }
                }
            });
        }
        client_processes.push(client);
    }

    // Wait for clients
    for mut c in client_processes {
        let status = c.wait().expect("Failed to wait on client");
        assert!(status.success(), "One of the clients failed");
    }

    // After clients finish, verify file contents on server side
    for i in 0..file_count {
        let absolute_remote_file_path = format!("{}\\{}", cwd.display(), remote_file_names[i]);
        assert!(Path::new(&absolute_remote_file_path).exists(),
            "Uploaded file does not exist on the server: {}",
            absolute_remote_file_path
        );

        let original_content = std::fs::read_to_string(&file_names[i]).expect("Failed to read original file");
        let uploaded_content = std::fs::read_to_string(&absolute_remote_file_path).expect("Failed to read uploaded file");
        assert_eq!(original_content, uploaded_content, "Content mismatch after test");

        remove_file(&file_names[i]).ok();
        remove_file(&absolute_remote_file_path).ok();
    }

    // On Unix, we don't have server_lines directly; we must read from the log file.
    #[cfg(unix)]
    {
        // Wait a bit to ensure server flushed logs
        sleep(Duration::from_secs(1));
        let server_content = read_to_string(_log_file_path).expect("Failed to read daemon log file");
        let server_lines: Vec<String> = server_content.lines().map(|s| s.to_string()).collect();
        verify_chunks(server_lines, file_count, transfer_rate);
        println!("Bandwidth distribution test passed successfully (Unix daemon mode).");
    }
}

// Extracted verification logic
fn verify_chunks(server_lines: Vec<String>, file_count: usize, transfer_rate: usize) {
    let expected_chunk = transfer_rate / file_count;

    let next_lines: Vec<String> = server_lines.iter()
        .filter(|l| l.contains("NEXT "))
        .map(|l| l.clone())
        .collect();

    let mut chunk_sizes = Vec::new();
    for line in &next_lines {
        if let Some(idx) = line.find("NEXT ") {
            let after = &line[idx+5..];
            let first_token = after.split_whitespace().next().unwrap_or("");
            let cleaned = first_token.trim_matches(|c: char| !c.is_ascii_digit());
            if let Ok(chunk_size) = cleaned.parse::<usize>() {
                chunk_sizes.push(chunk_size);
            }
        }
    }

    assert!(
        chunk_sizes.contains(&expected_chunk),
        "Expected chunk size {} not found in chunk sizes: {:?}",
        expected_chunk,
        chunk_sizes
    );
}