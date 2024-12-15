use std::path::{Path, PathBuf};
pub mod debug_utils;
mod err_utils;

pub use err_utils::{GetError, parse_server_response, ServerResponse};

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
    fn test_normalize_path_windows_absolute() {
        let input_path = "/Users/gabri/OneDrive/Área de Trabalho/odo/remcp_project/target/debug";
        let expected_path = r"C:\Users\gabri\OneDrive\Área de Trabalho\odo\remcp_project\target\debug";

        let normalized_path = normalize_path(input_path);
        assert_eq!(normalized_path.to_str().unwrap(), expected_path);
    }
}