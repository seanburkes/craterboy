fn main() {
    if std::env::args().any(|arg| arg == "--gui") {
        craterboy::interface::gui::run();
    } else {
        craterboy::interface::cli::run();
    }
}
