#[macro_export]
macro_rules! debugln {
    ($($fmt:tt)*) => {{
        print!("\x1b[34m");
        println!($($fmt)*);
        print!("\x1b[0m")
    }};
}
#[macro_export]
macro_rules! infoln {
    ($($fmt:tt)*) => {{
        print!("\x1b[92m");
        println!($($fmt)*);
        print!("\x1b[0m")
    }};
}
#[macro_export]
macro_rules! warnln {
    ($($fmt:tt)*) => {{
        print!("\x1b[93m");
        println!($($fmt)*);
        print!("\x1b[0m")
    }};
}
#[macro_export]
macro_rules! errorln {
    ($($fmt:tt)*) => {{
        print!("\x1b[91m");
        println!($($fmt)*);
        print!("\x1b[0m")
    }};
}
#[macro_export]
macro_rules! fatalln {
    ($($fmt:tt)*) => {{
        print!("\x1b[38;5;226m");
        println!($($fmt)*);
        print!("\x1b[0m")
    }};
}
