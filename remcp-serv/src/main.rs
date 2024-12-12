use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{Read, Write, BufRead, BufReader, BufWriter, Seek, SeekFrom};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::Duration;
use shared_lib::{GetError, normalize_path, debug_println, debug_eprintln};

const TRANSFER_RATE: usize = 256;
const MAX_CLIENTS: usize = 5;

static ACTIVE_CLIENTS: AtomicUsize = AtomicUsize::new(0);

fn send_error<W: Write>(writer: &mut W, err: GetError) -> std::io::Result<()> {
    eprintln!("Sending error to client: {}", err);
    writeln!(writer, "ERR {}", err)?;
    writer.flush()?;
    Ok(())
}

fn rate_limit(bytes_read: usize) {
    let active = ACTIVE_CLIENTS.load(Ordering::SeqCst);
    if active > 0 {
        let per_client_rate = std::cmp::max(1, TRANSFER_RATE / active);
        let delay_ms = (bytes_read * 1000) / per_client_rate;
        thread::sleep(Duration::from_millis(delay_ms as u64));
    }
}

fn calculate_chunk_size() -> usize {
    let active = ACTIVE_CLIENTS.load(Ordering::SeqCst);
    if active == 0 {
        return TRANSFER_RATE as usize;
    }
    let per_client_rate = std::cmp::max(1, TRANSFER_RATE / active);
    per_client_rate as usize
}

fn handle_get(
    reader: &mut BufReader<&TcpStream>,
    writer: &mut BufWriter<&TcpStream>,
    remote_path: &std::path::Path,
    offset: u64,
) -> std::io::Result<()> {
    let _ = reader;

    debug_println!("Handling GET request: path='{}', offset={}", remote_path.display(), offset);

    let mut file = match File::open(&remote_path) {
        Ok(f) => f,
        Err(e) => {
            debug_eprintln!("Failed to open file '{}': {}", remote_path.display(), e);
            send_error(writer, GetError::FileError(e.to_string()))?;
            return Ok(());
        }
    };

    let filesize = file.metadata()?.len();
    if offset >= filesize {
        debug_println!("Offset >= filesize. Sending 'OK 0'.");
        writeln!(writer, "OK 0")?;
        writer.flush()?;
        return Ok(());
    }

    file.seek(SeekFrom::Start(offset))?;
    let remaining = filesize - offset;
    writeln!(writer, "OK {}", remaining)?;
    writer.flush()?;
    debug_println!("Sent 'OK {}' to client for GET.", remaining);

    let mut total_sent = 0u64;
    while total_sent < remaining {
        let chunk_size = calculate_chunk_size();
        writeln!(writer, "NEXT {}", chunk_size)?;
        writer.flush()?;
        debug_println!("GET: Sent 'NEXT {}' to client.", chunk_size);

        let to_read = std::cmp::min(chunk_size as u64, remaining - total_sent) as usize;
        let mut buffer = vec![0u8; to_read];
        let bytes_read = file.read(&mut buffer)?;

        if bytes_read == 0 {
            debug_println!("File ended unexpectedly during GET. total_sent={} remaining={}.", total_sent, remaining);
            break;
        }

        writer.write_all(&buffer[..bytes_read])?;
        writer.flush()?;
        total_sent += bytes_read as u64;
        debug_println!("GET: Sent {} bytes. Total sent: {} / {}", bytes_read, total_sent, remaining);

        rate_limit(bytes_read);
    }

    debug_println!("File transfer complete for GET request.");
    Ok(())
}

fn handle_put(
    reader: &mut BufReader<&TcpStream>,
    writer: &mut BufWriter<&TcpStream>,
    remote_path: &std::path::Path,
    offset: u64,
    total_size: u64,
) -> std::io::Result<()> {
    debug_println!(
        "Handling PUT request: path='{}', offset={}, total_size={}",
        remote_path.display(),
        offset,
        total_size
    );

    if let Some(parent) = remote_path.parent() {
        if !parent.exists() {
            debug_println!("Creating directory '{}'", parent.display());
            create_dir_all(parent)?;
        }
    }

    let mut file = match OpenOptions::new().write(true).create(true).open(&remote_path) {
        Ok(f) => f,
        Err(e) => {
            debug_eprintln!("Failed to open file '{}': {}", remote_path.display(), e);
            send_error(writer, GetError::FileError(e.to_string()))?;
            return Ok(());
        }
    };

    file.seek(SeekFrom::Start(offset))?;
    writeln!(writer, "OK")?;
    writer.flush()?;
    debug_println!("Acknowledged PUT request. Ready to receive data.");

    let mut received = offset;
    while received < total_size {
        let chunk_size = calculate_chunk_size();
        writeln!(writer, "NEXT {}", chunk_size)?;
        writer.flush()?;
        debug_println!("PUT: Sent 'NEXT {}' to client.", chunk_size);

        let mut buffer = vec![0u8; chunk_size];
        let bytes_read = reader.read(&mut buffer)?;
        if bytes_read == 0 {
            debug_eprintln!(
                "Client closed connection prematurely. Received {} out of {} bytes.",
                received, total_size
            );
            break;
        }

        let bytes_to_write = std::cmp::min(bytes_read as u64, total_size - received) as usize;
        file.write_all(&buffer[..bytes_to_write])?;
        file.flush()?;
        received += bytes_to_write as u64;

        debug_println!("PUT: Received {} bytes. Total received: {} / {}", bytes_to_write, received, total_size);

        rate_limit(bytes_read);
    }

    if received == total_size {
        debug_println!("File upload complete for '{}'.", remote_path.display());
    } else {
        debug_eprintln!(
            "Upload incomplete for '{}'. Received {} out of {} bytes.",
            remote_path.display(),
            received,
            total_size
        );
    }

    Ok(())
}

fn handle_client(stream: TcpStream) -> std::io::Result<()> {
    let peer = stream.peer_addr()?;
    println!("New connection from {}", peer);

    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);

    let mut command = String::new();
    if reader.read_line(&mut command)? == 0 {
        debug_eprintln!("No command received from {}", peer);
        send_error(&mut writer, GetError::InvalidCommand)?;
        return Ok(());
    }

    let command = command.trim_end().to_string();
    debug_println!("Command received from {}: {}", peer, command);

    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        debug_eprintln!("Empty command from {}", peer);
        send_error(&mut writer, GetError::InvalidCommand)?;
        return Ok(());
    }

    let cmd = parts[0].to_uppercase();
    if cmd == "GET" {
        if parts.len() < 3 {
            debug_eprintln!("GET command missing arguments from {}", peer);
            send_error(&mut writer, GetError::MissingArguments)?;
            return Ok(());
        }
        let remote_path = normalize_path(parts[1]);
        let offset: u64 = parts[2].parse().unwrap_or(0);
        handle_get(&mut reader, &mut writer, &remote_path, offset)?;
    } else if cmd == "PUT" {
        if parts.len() < 4 {
            debug_eprintln!("PUT command missing arguments from {}", peer);
            send_error(&mut writer, GetError::MissingArguments)?;
            return Ok(());
        }
        let remote_path = normalize_path(parts[1]);
        let offset: u64 = parts[2].parse().unwrap_or(0);
        let total_size: u64 = parts[3].parse().unwrap_or(0);
        handle_put(&mut reader, &mut writer, &remote_path, offset, total_size)?;
    } else {
        debug_eprintln!("Unknown command '{}' from {}", cmd, peer);
        send_error(&mut writer, GetError::UnknownCommand)?;
    }

    debug_println!("Finished handling client {}", peer);
    Ok(())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() == 2 && args[1] == "--debug" {
        shared_lib::init_debug_mode(true);
    } else {
        shared_lib::init_debug_mode(false);
    }

    let listener = TcpListener::bind("0.0.0.0:7878")?;
    println!("Server running on port 7878");

    for stream in listener.incoming() {
        let stream = stream?;
        let current_clients = ACTIVE_CLIENTS.load(Ordering::SeqCst);

        if current_clients >= MAX_CLIENTS {
            debug_eprintln!("Maximum clients reached. Rejecting new connection.");
            let mut writer = BufWriter::new(&stream);
            writeln!(writer, "ERR Server is busy")?;
            writer.flush()?;
            continue;
        }

        ACTIVE_CLIENTS.fetch_add(1, Ordering::SeqCst);
        println!(
            "Client connected. Active clients: {}",
            ACTIVE_CLIENTS.load(Ordering::SeqCst)
        );

        thread::spawn(move || {
            let _ = handle_client(stream);
            ACTIVE_CLIENTS.fetch_sub(1, Ordering::SeqCst);
            println!(
                "Client disconnected. Active clients: {}",
                ACTIVE_CLIENTS.load(Ordering::SeqCst)
            );
        });
    }

    Ok(())
}