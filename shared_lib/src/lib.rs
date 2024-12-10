use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub enum GetError {
    InvalidCommand,
    MissingArguments,
    FileError(String),
    UnknownCommand,
    ServerBusy,
    Other(String),
}

impl fmt::Display for GetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GetError::InvalidCommand => write!(f, "Invalid command"),
            GetError::MissingArguments => write!(f, "Missing arguments"),
            GetError::FileError(err) => write!(f, "File error: {}", err),
            GetError::UnknownCommand => write!(f, "Unknown command"),
            GetError::ServerBusy => write!(f, "Server is busy"),
            GetError::Other(err) => write!(f, "Other error: {}", err),
        }
    }
}

impl std::error::Error for GetError {}

pub fn parse_error(response: &str) -> GetError {
    match response {
        "ERR Invalid command" => GetError::InvalidCommand,
        "ERR Missing arguments" => GetError::MissingArguments,
        "ERR Unknown command" => GetError::UnknownCommand,
        "ERR Server busy" => GetError::ServerBusy,
        _ if response.starts_with("ERR ") => GetError::FileError(response[4..].to_string()),
        _ => GetError::Other(response.to_string()),
    }
}

pub enum ServerResponse {
    Ok,
    Error(GetError),
}

impl ServerResponse {
    pub fn from_response(response: &str) -> Self {
        if response.starts_with("ERR") {
            ServerResponse::Error(parse_error(response))
        } else {
            ServerResponse::Ok
        }
    }
}

pub fn normalize_path(path: &str) -> PathBuf {
    if cfg!(windows) {
        if path.starts_with('/') {
            Path::new(&format!("C:{}", path.replace('/', "\\"))).to_path_buf()
        } else {
            Path::new(&path.replace('/', "\\")).to_path_buf()
        }
    } else {
        Path::new(path).to_path_buf()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_error() {
        assert_eq!(parse_error("ERR Invalid command").to_string(), "Invalid command");
        assert_eq!(parse_error("ERR Missing arguments").to_string(), "Missing arguments");
        assert_eq!(parse_error("ERR Server busy").to_string(), "Server is busy");
        assert_eq!(
            parse_error("ERR File not found").to_string(),
            "File error: File not found"
        );
        assert_eq!(
            parse_error("Unexpected error").to_string(),
            "Other error: Unexpected error"
        );
    }

    #[test]
    fn test_server_response() {
        match ServerResponse::from_response("ERR Invalid command") {
            ServerResponse::Error(err) => assert_eq!(err.to_string(), "Invalid command"),
            _ => panic!("Expected an error response"),
        }

        match ServerResponse::from_response("OK") {
            ServerResponse::Ok => {}
            _ => panic!("Expected an OK response"),
        }
    }

    #[test]
    fn test_normalize_path_windows_absolute() {
        let input_path = "/Users/gabri/OneDrive/Área de Trabalho/odo/remcp_project/target/debug";
        let expected_path = r"C:\Users\gabri\OneDrive\Área de Trabalho\odo\remcp_project\target\debug";

        let normalized_path = normalize_path(input_path);
        assert_eq!(normalized_path.to_str().unwrap(), expected_path);
    }
}