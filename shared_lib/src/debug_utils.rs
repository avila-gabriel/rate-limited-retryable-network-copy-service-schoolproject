use std::sync::OnceLock;

static DEBUG_MODE: OnceLock<bool> = OnceLock::new();

pub fn init_debug_mode(debug: bool) {
    let _ = DEBUG_MODE.set(debug);
}

pub fn is_debug() -> bool {
    *DEBUG_MODE.get_or_init(|| false)
}

#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        if $crate::debug_utils::is_debug() {
            println!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        if $crate::debug_utils::is_debug() {
            eprintln!($($arg)*);
        }
    };
}