fn main() {
    if let Err(e) = gluon_build::run() {
        eprintln!("gluon build failed: {e}");
        std::process::exit(1);
    }
}
