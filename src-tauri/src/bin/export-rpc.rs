fn main() {
    let runtime = tokio::runtime::Runtime::new().expect("contract export runtime");
    let _guard = runtime.enter();
    let router = hypnofy_lib::api::router();
    let tracked = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../src/shared/rpc/generated/bindings.ts");
    let check = std::env::args().any(|a| a == "--check");
    let temporary = tempfile::tempdir().expect("temporary binding check");
    let destination = if check {
        temporary.path().join("bindings.ts")
    } else {
        tracked.clone()
    };
    std::fs::create_dir_all(destination.parent().expect("bindings directory"))
        .expect("create bindings directory");
    taurpc::Exporter::new()
        .error_handling(taurpc::ErrorHandlingMode::Result)
        .export(&router, &destination)
        .expect("export typed contract");
    if check {
        let current = std::fs::read_to_string(&tracked)
            .expect("tracked bindings")
            .replace("\r\n", "\n");
        let generated = std::fs::read_to_string(&destination)
            .expect("generated bindings")
            .replace("\r\n", "\n");
        assert_eq!(
            current, generated,
            "Bindings are stale; run npm run rpc:generate"
        );
        println!("Generated IPC contract matches Rust");
    }
}
