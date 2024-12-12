use std::fmt;

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
    Next(usize),
}

impl ServerResponse {
    pub fn from_response(response: &str) -> Self {
        if response.starts_with("ERR") {
            ServerResponse::Error(parse_error(response))
        } else if response.starts_with("OK") {
            ServerResponse::Ok
        } else if response.starts_with("NEXT ") {
            let parts: Vec<&str> = response.split_whitespace().collect();
            if parts.len() == 2 {
                if let Ok(sz) = parts[1].parse::<usize>() {
                    return ServerResponse::Next(sz);
                }
            }
            ServerResponse::Error(GetError::Other("Invalid NEXT command format".to_string()))
        } else {
            ServerResponse::Error(GetError::Other("Invalid response".to_string()))
        }
    }
}

pub fn parse_server_response(line: &str) -> ServerResponse {
    if line.starts_with("ERR ") {
        let err_str = &line[4..];
        if err_str == "Invalid command" {
            ServerResponse::Error(GetError::InvalidCommand)
        } else if err_str == "Missing arguments" {
            ServerResponse::Error(GetError::MissingArguments)
        } else if err_str == "Unknown command" {
            ServerResponse::Error(GetError::UnknownCommand)
        } else if err_str == "Server is busy" {
            ServerResponse::Error(GetError::ServerBusy)
        } else {
            ServerResponse::Error(GetError::Other(err_str.to_string()))
        }
    } else if line.starts_with("OK") {
        ServerResponse::Ok
    } else if line.starts_with("NEXT ") {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() == 2 {
            if let Ok(sz) = parts[1].parse::<usize>() {
                return ServerResponse::Next(sz);
            }
        }
        ServerResponse::Error(GetError::Other("Invalid NEXT command format".to_string()))
    } else {
        ServerResponse::Error(GetError::Other("Invalid response".to_string()))
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
    fn test_server_response_next() {
        match ServerResponse::from_response("NEXT 64") {
            ServerResponse::Next(size) => assert_eq!(size, 64),
            _ => panic!("Expected a NEXT 64 response"),
        }

        match ServerResponse::from_response("NEXT abc") {
            ServerResponse::Error(err) => {
                assert_eq!(err.to_string(), "Other error: Invalid NEXT command format")
            }
            _ => panic!("Expected an error for invalid NEXT format"),
        }

        match ServerResponse::from_response("UNKNOWN") {
            ServerResponse::Error(err) => {
                assert!(err.to_string().contains("Invalid response"))
            },
            _ => panic!("Expected invalid response error"),
        }
    }

    #[test]
    fn test_server_response_ok_and_err() {
        match ServerResponse::from_response("ERR Invalid command") {
            ServerResponse::Error(err) => assert_eq!(err.to_string(), "Invalid command"),
            _ => panic!("Expected an error response"),
        }

        match ServerResponse::from_response("OK") {
            ServerResponse::Ok => {}
            _ => panic!("Expected an OK response"),
        }
    }
}