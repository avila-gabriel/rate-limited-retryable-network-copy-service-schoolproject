use std::fs::{File, OpenOptions, create_dir_all};
use std::io::{Read, Write, BufRead, BufReader, BufWriter, Seek, SeekFrom};
use std::net::{TcpListener, TcpStream};
use std::sync::{
    atomic::{AtomicUsize, AtomicU64, Ordering},
    Arc,
};
use std::thread;
use std::time::Duration;
use shared_lib::{GetError, normalize_path};

const TRANSFER_RATE: u64 = 256;
const MAX_CLIENTS: usize = 5;

static ACTIVE_CLIENTS: AtomicUsize = AtomicUsize::new(0);

fn send_error<W: Write>(writer: &mut W, err: GetError) -> std::io::Result<()> {
    eprintln!("Sending error to client: {}", err);
    writeln!(writer, "ERR {}", err)?;
    writer.flush()?;
    Ok(())
}

fn handle_client(
    stream: TcpStream,
    shared_rate: Arc<AtomicU64>,
) -> std::io::Result<()> {
    let peer = stream.peer_addr()?;
    println!("New connection from {}", peer);

    let mut reader = BufReader::new(&stream);
    let mut writer = BufWriter::new(&stream);

    // Read the command line from the client
    let mut command = String::new();
    if reader.read_line(&mut command)? == 0 {
        eprintln!("Client {} sent no command.", peer);
        send_error(&mut writer, GetError::InvalidCommand)?;
        return Ok(());
    }
    let command = command.trim_end().to_string();
    println!("Client {} command received: {}", peer, command);

    let parts: Vec<&str> = command.split_whitespace().collect();
    if parts.is_empty() {
        eprintln!("Client {} sent an empty command.", peer);
        send_error(&mut writer, GetError::InvalidCommand)?;
        return Ok(());
    }
    let cmd = parts[0].to_uppercase();

    if cmd == "GET" {
        if parts.len() < 3 {
            eprintln!("Client {} GET command missing arguments.", peer);
            send_error(&mut writer, GetError::MissingArguments)?;
            return Ok(());
        }

        let remote_path = normalize_path(parts[1]);
        println!("---------------------- {:?}", remote_path);
        let offset: u64 = parts[2].parse().unwrap_or(0);

        println!(
            "Client {} requested GET: path='{}', offset={}",
            peer,
            remote_path.display(),
            offset
        );

        let mut file = match File::open(&remote_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to open file '{}': {}", remote_path.display(), e);
                send_error(&mut writer, GetError::FileError(e.to_string()))?;
                return Ok(());
            }
        };

        let filesize = file.metadata()?.len();
        if offset >= filesize {
            println!("Client {} requested file already fully transferred.", peer);
            writeln!(writer, "OK 0")?;
            writer.flush()?;
            return Ok(());
        }

        file.seek(SeekFrom::Start(offset))?;
        writeln!(writer, "OK {}", filesize - offset)?;
        writer.flush()?;
        println!(
            "Client {} file transfer initiated. Remaining size: {}",
            peer, filesize - offset
        );

        let mut buffer = [0u8; 128];
        let rate_for_client = Arc::clone(&shared_rate);
        while let Ok(bytes_read) = file.read(&mut buffer) {
            if bytes_read == 0 {
                println!("Client {} file transfer completed.", peer);
                break;
            }
            writer.write_all(&buffer[..bytes_read])?;
            writer.flush()?;

            let current_rate = rate_for_client.load(Ordering::SeqCst);
            let bytes_transferred = bytes_read as u64;
            println!("Client {} sent {} bytes.", peer, bytes_transferred);

            if current_rate > 0 {
                thread::sleep(Duration::from_millis(
                    (bytes_transferred * 1000) / current_rate,
                ));
            }
        }
    } else if cmd == "PUT" {
        if parts.len() < 4 {
            eprintln!("Client {} PUT command missing arguments.", peer);
            send_error(&mut writer, GetError::MissingArguments)?;
            return Ok(());
        }

        let remote_path = normalize_path(parts[1]);
        println!("---------------------- {:?}", remote_path);
        let offset: u64 = parts[2].parse().unwrap_or(0);
        let total_size: u64 = parts[3].parse().unwrap_or(0);

        println!(
            "Client {} requested PUT: path='{}', offset={}, total_size={}",
            peer,
            remote_path.display(),
            offset,
            total_size
        );

        if let Some(parent) = remote_path.parent() {
            if !parent.exists() {
                create_dir_all(parent)?;
            }
        }

        let mut file = match OpenOptions::new().write(true).create(true).open(&remote_path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("Failed to open file '{}': {}", remote_path.display(), e);
                send_error(&mut writer, GetError::FileError(e.to_string()))?;
                return Ok(());
            }
        };

        file.seek(SeekFrom::Start(offset))?;
        writeln!(writer, "OK")?;
        writer.flush()?;
        println!("Client {} acknowledged PUT request. Ready to receive data.", peer);

        let mut received: u64 = offset;
        let rate_for_client = Arc::clone(&shared_rate);
        let mut buffer = [0u8; 128];
        while received < total_size {
            let to_read = std::cmp::min(buffer.len() as u64, total_size - received) as usize;
            let bytes_read = reader.read(&mut buffer[..to_read])?;
            if bytes_read == 0 {
                eprintln!(
                    "Client {} connection closed prematurely. Received {} of {} bytes.",
                    peer, received, total_size
                );
                break;
            }
            file.write_all(&buffer[..bytes_read])?;
            file.flush()?;
            received += bytes_read as u64;
            println!("Client {} received {} bytes. Total: {} / {}", peer, bytes_read, received, total_size);

            let current_rate = rate_for_client.load(Ordering::SeqCst);
            if current_rate > 0 {
                thread::sleep(Duration::from_millis(
                    (bytes_read as u64 * 1000) / current_rate,
                ));
            }
        }

        if received == total_size {
            println!("Client {} completed file upload.", peer);
        } else {
            eprintln!(
                "Client {} upload incomplete. Received {} out of {} bytes.",
                peer, received, total_size
            );
        }
    } else {
        eprintln!("Client {} sent unknown command: {}", peer, cmd);
        send_error(&mut writer, GetError::UnknownCommand)?;
    }

    println!("Done handling client {}", peer);
    Ok(())
}

fn main() -> std::io::Result<()> {
    let listener = TcpListener::bind("0.0.0.0:7878")?;
    let shared_rate = Arc::new(AtomicU64::new(TRANSFER_RATE));

    {
        let rate_limiter = Arc::clone(&shared_rate);
        thread::spawn(move || loop {
            let active_clients = ACTIVE_CLIENTS.load(Ordering::SeqCst);
            let new_rate = if active_clients > 0 {
                TRANSFER_RATE / active_clients as u64
            } else {
                TRANSFER_RATE
            };
            println!(
                "Adjusting transfer rate to {} bytes/sec for {} active clients.",
                new_rate, active_clients
            );
            rate_limiter.store(new_rate, Ordering::SeqCst);
            thread::sleep(Duration::from_secs(1));
        });
    }

    println!("Server running on port 7878");
    for stream in listener.incoming() {
        let stream = stream?;
        let current_clients = ACTIVE_CLIENTS.load(Ordering::SeqCst);
        if current_clients >= MAX_CLIENTS {
            eprintln!("Connection rejected: server is busy. Active clients: {}", current_clients);
            let mut writer = BufWriter::new(&stream);
            writeln!(writer, "ERR {}", GetError::ServerBusy)?;
            writer.flush()?;
            continue;
        }

        ACTIVE_CLIENTS.fetch_add(1, Ordering::SeqCst);
        println!(
            "Client connected. Active clients: {}",
            ACTIVE_CLIENTS.load(Ordering::SeqCst)
        );

        let rate_ref = Arc::clone(&shared_rate);
        thread::spawn(move || {
            let _ = handle_client(stream, rate_ref);
            ACTIVE_CLIENTS.fetch_sub(1, Ordering::SeqCst);
            println!(
                "Client disconnected. Active clients: {}",
                ACTIVE_CLIENTS.load(Ordering::SeqCst)
            );
        });
    }

    Ok(())
}