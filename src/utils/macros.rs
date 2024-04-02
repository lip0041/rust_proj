#[macro_export]
macro_rules! debug {
    ($($arg:tt)*) => {{
        let collect = stdext::function_name!().split_terminator("::").collect::<Vec<&str>>();

        let mut f = collect[collect.len() - 1];
        if collect[collect.len() - 1].contains("closure") {
            f = collect[collect.len() - 2];
        };
        print!("\x1b[34m{} D {:?} {:?}: [{}()] [{}:{}] {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), f, file!(), line!(), format_args!($($arg)*));
    }};
}
#[macro_export]
macro_rules! info {
    ($($arg:tt)*) => {{
        let collect = stdext::function_name!().split_terminator("::").collect::<Vec<&str>>();

        let mut f = collect[collect.len() - 1];
        if collect[collect.len() - 1].contains("closure") {
            f = collect[collect.len() - 2];
        };
        print!("\x1b[92m{} D {:?} {:?}: [{}()] [{}:{}] {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), f, file!(), line!(), format_args!($($arg)*));
    }};
}
#[macro_export]
macro_rules! warn {
    ($($arg:tt)*) => {{
        let collect = stdext::function_name!().split_terminator("::").collect::<Vec<&str>>();

        let mut f = collect[collect.len() - 1];
        if collect[collect.len() - 1].contains("closure") {
            f = collect[collect.len() - 2];
        };
        print!("\x1b[93m{} D {:?} {:?}: [{}()] [{}:{}] {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), f, file!(), line!(), format_args!($($arg)*));
    }};
}
#[macro_export]
macro_rules! error {
    ($($arg:tt)*) => {{
        let collect = stdext::function_name!().split_terminator("::").collect::<Vec<&str>>();

        let mut f = collect[collect.len() - 1];
        if collect[collect.len() - 1].contains("closure") {
            f = collect[collect.len() - 2];
        };
        print!("\x1b[91m{} D {:?} {:?}: [{}()] [{}:{}] {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), f, file!(), line!(), format_args!($($arg)*));
    }};
}

#[macro_export]
macro_rules! fatal {
    ($($arg:tt)*) => {{
        let collect = stdext::function_name!().split_terminator("::").collect::<Vec<&str>>();

        let mut f = collect[collect.len() - 1];
        if collect[collect.len() - 1].contains("closure") {
            f = collect[collect.len() - 2];
        };
        print!("\x1b[38;5;226m{} D {:?} {:?}: [{}()] [{}:{}] {:?}\n", chrono::prelude::Local::now().format("%Y-%m-%d %H:%M:%S.%6f"),
            std::process::id(), std::thread::current().id(), f, file!(), line!(), format_args!($($arg)*));
    }};
}
