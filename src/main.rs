fn main() {
    if let Err(err) = kg::run(std::env::args_os(), &std::env::current_dir().expect("cwd")) {
        eprintln!("error: {}", kg::format_error_chain(&err));
        std::process::exit(1);
    }
}
