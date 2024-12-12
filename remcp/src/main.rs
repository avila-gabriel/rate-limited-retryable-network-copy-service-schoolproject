use std::env;
use std::fs::{File, OpenOptions, rename};
use std::io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process;
use shared_lib::{parse_server_response, normalize_path, ServerResponse, debug_println, debug_eprintln};

fn split_host_path(remote: &str) -> (String, String) {
    if let Some(idx) = remote.find(':') {
        let host = &remote[..idx];
        let path = &remote[idx + 1..];
        return (host.to_string(), path.to_string());
    }
    (remote.to_string(), ":".to_string())
}

fn determine_offset_and_part_path(local_path: &Path) -> (u64, std::path::PathBuf) {
    let part_path = local_path.with_extension("part");
    let offset = if let Ok(metadata) = std::fs::metadata(&part_path) {
        metadata.len()
    } else {
        0
    };
    (offset, part_path)
}

fn do_get(remote_host: &str, remote_path: &str, local_path: &Path) -> std::io::Result<()> {
    let (offset, part_path) = determine_offset_and_part_path(local_path);

    debug_println!("Starting GET operation from '{}' to local path '{}', offset={}", remote_host, local_path.display(), offset);
    let addr = format!("{}:7878", remote_host);
    let stream = TcpStream::connect(&addr)?;
    debug_println!("Connected to server at '{}'", addr);

    let mut writer = BufWriter::new(&stream);
    writeln!(writer, "GET {} {}", remote_path, offset)?;
    writer.flush()?;
    debug_println!("Sent GET command: path='{}', offset={}", remote_path, offset);

    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response)?;
    let response = response.trim_end();
    debug_println!("Server response: '{}'", response);

    match parse_server_response(response) {
        ServerResponse::Error(err) => {
            eprintln!("Error received from server: {}", err);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", err)));
        },
        ServerResponse::Ok => {
            let parts: Vec<&str> = response.split_whitespace().collect();
            if parts.len() < 2 {
                return Err(std::io::Error::new(std::io::ErrorKind::Other, "Invalid server response format"));
            }
            let remaining_size: u64 = parts[1].parse().unwrap_or(0);
            debug_println!("Remaining size to download: {}", remaining_size);

            if remaining_size == 0 {
                println!("No data to download.");
                return Ok(());
            }

            let mut file = OpenOptions::new().write(true).create(true).open(&part_path)?;
            file.seek(SeekFrom::Start(offset))?;
            debug_println!("Opened partial file '{}', resuming at offset {}", part_path.display(), offset);

            let mut received = 0u64;

            while received < remaining_size {
                let mut line = String::new();
                if reader.read_line(&mut line)? == 0 {
                    eprintln!("Server closed connection unexpectedly during GET.");
                    return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Server closed connection"));
                }
                let line = line.trim_end();
                debug_println!("Server 'NEXT' response: '{}'", line);

                match parse_server_response(line) {
                    ServerResponse::Next(chunk_size) => {
                        let to_read = std::cmp::min(chunk_size as u64, remaining_size - received) as usize;
                        let mut buffer = vec![0u8; to_read];

                        let bytes_read = reader.read_exact(&mut buffer).map(|_| to_read).or_else(|e| {
                            if e.kind() == std::io::ErrorKind::UnexpectedEof {
                                let got = buffer.len() - reader.buffer().len();
                                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, format!("Connection lost, got {} instead of {}", got, to_read)));
                            }
                            Err(e)
                        })?;

                        file.write_all(&buffer[..bytes_read])?;
                        file.flush()?;
                        received += bytes_read as u64;
                        debug_println!("Received {} bytes. Total received: {} / {}", bytes_read, received, remaining_size);
                    },
                    ServerResponse::Ok => {
                        debug_eprintln!("Unexpected 'OK' before finishing GET download.");
                        break;
                    },
                    ServerResponse::Error(err) => {
                        eprintln!("Error received from server during GET: {}", err);
                        return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", err)));
                    }
                }
            }

            if received == remaining_size {
                debug_println!("Download complete. Renaming part file to final file.");
                rename(part_path, local_path)?;
            } else {
                eprintln!("Incomplete download. Received {} bytes out of {}.", received, remaining_size);
                return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Incomplete download"));
            }
        },
        ServerResponse::Next(_) => {
            eprintln!("Unexpected 'NEXT' response in GET operation.");
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Unexpected 'NEXT' in GET"));
        }
    }

    println!("GET operation completed successfully.");
    Ok(())
}

fn do_put(remote_host: &str, remote_path: &str, local_path: &Path) -> std::io::Result<()> {
    let (offset, part_path) = determine_offset_and_part_path(local_path);

    debug_println!("Starting PUT operation: local file '{}' to remote path '{}:{}', offset={}", local_path.display(), remote_host, remote_path, offset);
    let total_size = std::fs::metadata(local_path)?.len();
    debug_println!("File size: {} bytes", total_size);

    let addr = format!("{}:7878", remote_host);
    let stream = TcpStream::connect(&addr)?;
    debug_println!("Connected to server at '{}'", addr);

    let mut writer = BufWriter::new(&stream);
    writeln!(writer, "PUT {} {} {}", remote_path, offset, total_size)?;
    writer.flush()?;
    debug_println!("Sent PUT command: path='{}', offset={}, total_size={}", remote_path, offset, total_size);

    let mut reader = BufReader::new(&stream);
    let mut line = String::new();
    reader.read_line(&mut line)?;
    let mut line = line.trim_end().to_string();
    debug_println!("Server initial response: '{}'", line);

    match parse_server_response(&line) {
        ServerResponse::Error(err) => {
            eprintln!("Error received from server: {}", err);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", err)));
        },
        ServerResponse::Ok => debug_println!("Server acknowledged PUT request. Starting file upload."),
        _ => {
            eprintln!("Unexpected server response: '{}'", line);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, "Invalid server response"));
        }
    }

    let mut file = File::open(local_path)?;
    file.seek(SeekFrom::Start(offset))?;
    let mut part_file = OpenOptions::new().write(true).create(true).open(&part_path)?;
    part_file.seek(SeekFrom::Start(offset))?;
    debug_println!("Prepared partial file at '{}', resuming at offset {}", part_path.display(), offset);

    let mut sent = offset;

    while sent < total_size {
        line.clear();
        if reader.read_line(&mut line)? == 0 {
            eprintln!("Server closed connection unexpectedly during PUT.");
            return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Server closed connection"));
        }
        let line_buf = line.trim_end();
        debug_println!("Server 'NEXT' response: '{}'", line_buf);

        match parse_server_response(line_buf) {
            ServerResponse::Next(chunk_size) => {
                let remaining = total_size - sent;
                let to_read = std::cmp::min(chunk_size as u64, remaining) as usize;
                let mut buffer = vec![0u8; to_read];
                let bytes_read = file.read(&mut buffer)?;
                if bytes_read == 0 {
                    debug_eprintln!("No more data to send but server expects more. Sent so far: {} bytes.", sent);
                    break;
                }
                writer.write_all(&buffer[..bytes_read])?;
                writer.flush()?;
                part_file.write_all(&buffer[..bytes_read])?;
                part_file.flush()?;
                sent += bytes_read as u64;
                debug_println!("Sent {} bytes. Total sent: {} / {}", bytes_read, sent, total_size);
            },
            ServerResponse::Ok => {
                debug_println!("Server acknowledged file transfer completion.");
                break;
            },
            ServerResponse::Error(err) => {
                eprintln!("Error received from server: {}", err);
                return Err(std::io::Error::new(std::io::ErrorKind::Other, format!("{}", err)));
            }
        }
    }

    if sent == total_size {
        debug_println!("Upload complete. Removing part file '{}'.", part_path.display());
        std::fs::remove_file(part_path).ok();
    } else {
        eprintln!("Upload incomplete. Sent {} bytes out of {}.", sent, total_size);
        return Err(std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "Upload incomplete"));
    }

    println!("PUT operation completed successfully.");
    Ok(())
}

fn main() {
    let args: Vec<String> = env::args().collect();
    let mut positional_args = vec![];

    for arg in args.iter().skip(1) {
        if arg == "--debug" {
            shared_lib::init_debug_mode(true);
            debug_println!("Debug mode enabled.");
        } else {
            positional_args.push(arg.clone());
        }
    }
    if !args.iter().any(|a| a == "--debug") {
        shared_lib::init_debug_mode(false);
    }

    if positional_args.len() != 2 {
        eprintln!("Usage: {} [--debug] <source> <destination>", args[0]);
        process::exit(1);
    }

    let src = positional_args[0].clone();
    let dst = positional_args[1].clone();

    let is_src_remote = src.contains(":");
    let is_dst_remote = dst.contains(":");

    if is_src_remote && is_dst_remote {
        eprintln!("Error: Both source and destination cannot be remote.");
        process::exit(1);
    }

    if !is_src_remote && !is_dst_remote {
        eprintln!("Error: Both source and destination cannot be local.");
        process::exit(1);
    }

    if is_src_remote {
        let (host, remote_path) = split_host_path(&src);
        let local_path = normalize_path(&dst);
        if let Err(e) = do_get(&host, &remote_path, &local_path) {
            eprintln!("GET operation failed: {}", e);
        } else {
            println!("GET operation succeeded.");
        }
    } else {
        let (host, remote_path) = split_host_path(&dst);
        let local_path = normalize_path(&src);
        if let Err(e) = do_put(&host, &remote_path, &local_path) {
            eprintln!("PUT operation failed: {}", e);
        } else {
            println!("PUT operation succeeded.");
        }
    }
}