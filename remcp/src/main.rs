use std::env;
use std::fs::{File, OpenOptions, rename};
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use shared_lib::{ServerResponse, normalize_path};
use std::path::Path;

fn split_host_path(remote: &str) -> (String, String) {
    if let Some(idx) = remote.find(':') {
        let host = &remote[..idx];
        let path = &remote[idx + 1..];
        return (host.to_string(), path.to_string());
    }
    (remote.to_string(), ":".to_string())
}

fn try_get(
    remote_host: &str,
    remote_path: &str,
    local_path_str: &str,
    max_retries: usize,
) -> std::io::Result<()> {
    let local_path = normalize_path(local_path_str);
    let part_path_str = format!("{}.part", local_path.to_string_lossy());
    let part_path = normalize_path(&part_path_str);
    println!("---------------------- {:?}, {:?}", local_path, part_path);

    let offset = if let Ok(metadata) = std::fs::metadata(&part_path) {
        metadata.len()
    } else {
        0
    };

    println!("Starting GET operation for '{}' from '{}' to '{}'.", remote_path, remote_host, local_path.display());
    println!("Partial file path: '{}', Offset: {}", part_path.display(), offset);

    let mut attempt = 0;
    loop {
        attempt += 1;
        println!("Attempt {}/{} to GET the file...", attempt, max_retries);
        match do_get(remote_host, remote_path, &local_path, &part_path, offset) {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error during GET operation: {}", e);
                if attempt >= max_retries {
                    eprintln!("Exceeded maximum retries. Aborting GET operation.");
                    return Err(e);
                }
                if e.to_string().contains("Server is busy") {
                    eprintln!("Server is busy. Retrying in 2 seconds...");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    eprintln!("Unexpected error: {}. Aborting.", e);
                    return Err(e);
                }
            }
        }
    }
}

fn do_get(
    remote_host: &str,
    remote_path: &str,
    local_path: &Path,
    part_path: &Path,
    offset: u64,
) -> std::io::Result<()> {
    println!("Connecting to server at '{}'.", remote_host);
    let addr = format!("{}:7878", remote_host);
    let stream = TcpStream::connect(&addr)?;
    println!("Connected to server.");

    {
        let mut writer = BufWriter::new(&stream);
        writeln!(writer, "GET {} {}", remote_path, offset)?;
        writer.flush()?;
        println!("GET command sent: 'GET {} {}'", remote_path, offset);
    }

    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    let response = response.trim_end();
    println!("Server response: '{}'", response);

    match ServerResponse::from_response(response) {
        ServerResponse::Error(err) => {
            eprintln!("Server returned an error: {}", err);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, err));
        }
        ServerResponse::Ok => {
            println!("Server accepted GET request. Processing response...");
            let parts: Vec<&str> = response.split_whitespace().collect();
            if parts.len() < 2 || parts[0] != "OK" {
                eprintln!("Invalid response format.");
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Invalid response"));
            }
            let remaining_size: u64 = parts[1].parse().unwrap_or(0);

            if remaining_size == 0 {
                println!("No data to download. Completing operation.");
                if offset > 0 && part_path.exists() {
                    std::fs::rename(part_path, local_path)?;
                }
                return Ok(());
            }

            let mut file = OpenOptions::new().write(true).create(true).open(&part_path)?;
            file.seek(SeekFrom::Start(offset))?;
            println!("Resuming download at offset {}. Total size to download: {}", offset, remaining_size);

            let mut buf = [0u8; 128];
            let mut received: u64 = 0;
            while received < remaining_size {
                let to_read = std::cmp::min(buf.len() as u64, remaining_size - received) as usize;
                let bytes_read = reader.read(&mut buf[..to_read])?;
                if bytes_read == 0 {
                    eprintln!("Connection lost during download.");
                    break;
                }
                file.write_all(&buf[..bytes_read])?;
                file.flush()?;
                received += bytes_read as u64;
                println!("Received {} bytes. Total received: {} / {}", bytes_read, received, remaining_size);
            }

            if received == remaining_size {
                println!("Download complete. Renaming part file to final name.");
                rename(part_path, local_path)?;
            } else {
                eprintln!("Download incomplete. Connection lost.");
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Connection lost"));
            }
        }
    }

    Ok(())
}

fn try_put(
    local_path_str: &str,
    remote_host: &str,
    remote_path: &str,
    max_retries: usize,
) -> std::io::Result<()> {
    let local_path = normalize_path(local_path_str);
    let part_path_str = format!("{}.part", local_path.to_string_lossy());
    let part_path = normalize_path(&part_path_str);
    println!("---------------------- {:?}, {:?}", local_path, part_path);

    let offset = if let Ok(metadata) = std::fs::metadata(&part_path) {
        metadata.len()
    } else {
        0
    };

    let total_size = std::fs::metadata(&local_path)?.len();

    println!("Starting PUT operation for '{}' to '{}' at '{}'.", local_path.display(), remote_host, remote_path);
    println!("Partial file path: '{}', Offset: {}, Total size: {}", part_path.display(), offset, total_size);

    let mut attempt = 0;
    loop {
        attempt += 1;
        println!("Attempt {}/{} to PUT the file...", attempt, max_retries);
        match do_put(&local_path, remote_host, remote_path, &part_path, offset, total_size) {
            Ok(_) => {
                return Ok(());
            }
            Err(e) => {
                eprintln!("Error during PUT operation: {}", e);
                if attempt >= max_retries {
                    eprintln!("Exceeded maximum retries. Aborting PUT operation.");
                    return Err(e);
                }
                if e.to_string().contains("Server is busy") {
                    eprintln!("Server is busy. Retrying in 2 seconds...");
                    thread::sleep(Duration::from_secs(2));
                } else {
                    eprintln!("Unexpected error: {}. Retrying in 2 seconds.", e);
                    thread::sleep(Duration::from_secs(2));
                }
            }
        }
    }
}

fn do_put(
    local_path: &Path,
    remote_host: &str,
    remote_path: &str,
    part_path: &Path,
    offset: u64,
    total_size: u64,
) -> std::io::Result<()> {
    println!("Connecting to server at '{}'.", remote_host);
    let addr = format!("{}:7878", remote_host);
    let stream = TcpStream::connect(&addr)?;
    println!("Connected to server.");

    {
        let mut writer = BufWriter::new(&stream);
        writeln!(writer, "PUT {} {} {}", remote_path, offset, total_size)?;
        writer.flush()?;
        println!("PUT command sent: 'PUT {} {} {}'", remote_path, offset, total_size);
    }

    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    let response = response.trim_end();
    println!("Server response: '{}'", response);

    match ServerResponse::from_response(response) {
        ServerResponse::Error(err) => {
            eprintln!("Server returned an error: {}", err);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, err));
        }
        ServerResponse::Ok => {
            println!("Server accepted PUT request. Starting upload...");
        }
    }

    let mut file = File::open(local_path)?;
    file.seek(SeekFrom::Start(offset))?;

    let mut part_file = OpenOptions::new().write(true).create(true).open(&part_path)?;
    part_file.seek(SeekFrom::Start(offset))?;

    let mut buf = [0u8; 128];
    let mut sent = offset;
    let mut writer = BufWriter::new(&stream);
    while sent < total_size {
        let to_read = std::cmp::min(buf.len() as u64, total_size - sent) as usize;
        let bytes_read = file.read(&mut buf[..to_read])?;
        if bytes_read == 0 {
            eprintln!("Connection lost during upload.");
            break;
        }
        writer.write_all(&buf[..bytes_read])?;
        writer.flush()?;
        sent += bytes_read as u64;

        part_file.write_all(&buf[..bytes_read])?;
        part_file.flush()?;
        //println!("Sent {} bytes. Total sent: {} / {}", bytes_read, sent, total_size);
    }

    if sent == total_size {
        println!("Upload complete. Removing part file.");
        std::fs::remove_file(part_path).ok();
    } else {
        eprintln!("Upload incomplete. Connection lost.");
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Connection lost"));
    }

    Ok(())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = env::args().collect();
    if args.len() != 3 {
        eprintln!("Usage: {} <source> <destination>", args[0]);
        std::process::exit(1);
    }

    let src = &args[1];
    let dst = &args[2];
    let is_src_remote = src[2..].contains(":");
    let is_dst_remote = dst[2..].contains(":");

    if is_src_remote && is_dst_remote {
        eprintln!("Error: Both source and destination cannot be remote.");
        std::process::exit(1);
    }

    if !is_src_remote && !is_dst_remote {
        eprintln!("Error: Both source and destination cannot be local.");
        std::process::exit(1);
    }

    let max_retries = 5;

    if is_src_remote {
        println!("Initiating GET operation...");
        let (host, remote_path) = split_host_path(src);
        let local_path = dst;
        match try_get(&host, &remote_path, local_path, max_retries) {
            Ok(_) => println!("GET operation completed successfully."),
            Err(e) => eprintln!("GET operation failed: {}", e),
        }
    } else {
        println!("Initiating PUT operation...");
        let local_path = src;
        let (host, remote_path) = split_host_path(dst);
        match try_put(local_path, &host, &remote_path, max_retries) {
            Ok(_) => println!("PUT operation completed successfully."),
            Err(e) => eprintln!("PUT operation failed: {}", e),
        }
    }

    Ok(())
}