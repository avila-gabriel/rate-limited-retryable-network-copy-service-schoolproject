pub static mut DEBUG_MODE: bool = false;

#[macro_export]
macro_rules! debug_println {
    ($($arg:tt)*) => {
        if unsafe { $crate::debug_utils::DEBUG_MODE } {
            println!($($arg)*);
        }
    };
}

#[macro_export]
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {
        if unsafe { $crate::debug_utils::DEBUG_MODE } {
            eprintln!($($arg)*);
        }
    };
}