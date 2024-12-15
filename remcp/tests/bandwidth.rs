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

    let file_count = 5;
    let transfer_rate = 256;
    let expected_chunk = transfer_rate / file_count as u64;

    let file_names: Vec<String> = (0..file_count).map(|i| format!("test_upload_{}.txt", i)).collect();
    for f_name in &file_names {
        let mut f = File::create(f_name).expect("Failed to create test file");
        for i in 0..64 {
            writeln!(f, "Line {}", i).expect("Failed to write test file");
        }
    }

    let remote_file_names: Vec<String> = (0..file_count).map(|i| format!("test_upload_remote_{}.txt", i)).collect();

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

    for mut c in client_processes {
        let status = c.wait().expect("Failed to wait on client");
        assert!(status.success(), "One of the clients failed");
    }

    server.kill().ok();
    let _ = server.wait().ok();

    let server_lines = server_handle.join().expect("Failed to join server handle");

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
            if let Ok(chunk_size) = cleaned.parse::<u64>() {
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

    println!("Bandwidth distribution test passed successfully.");
}