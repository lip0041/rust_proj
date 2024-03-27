#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        print!("\x1b[34m{} D {:?} {:?} [{}:{}]: {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), file!(), line!(), format_args!($($arg)*));
    }};
}
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        print!("\x1b[92m{} D {:?} {:?} [{}:{}]: {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), file!(), line!(), format_args!($($arg)*));
    }};
}
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        print!("\x1b[93m{} D {:?} {:?} [{}:{}]: {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), file!(), line!(), format_args!($($arg)*));
    }};
}
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        print!("\x1b[91m{} D {:?} {:?} [{}:{}]: {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), file!(), line!(), format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! fatal {
    ($($arg:tt)*) => {{
        print!("\x1b[38;5;226m{} D {:?} {:?} [{}:{}]: {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), file!(), line!(), format_args!($($arg)*));
    }};
}
