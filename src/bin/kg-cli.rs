fn main() {
    if let Err(err) = kg::run(std::env::args_os(), &std::env::current_dir().expect("cwd")) {
        eprintln!("error: {err}");
        std::process::exit(1);
    }
}
