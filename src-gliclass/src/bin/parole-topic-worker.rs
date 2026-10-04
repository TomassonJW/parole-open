fn main() {
    // Ne jamais exposer les diagnostics internes, susceptibles de contenir des chemins.
    std::panic::set_hook(Box::new(|_| {}));
    if parole_gliclass::worker_service::child_main().is_err() {
        std::process::exit(1);
    }
}
