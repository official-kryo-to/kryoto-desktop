//! Downloads public emulator releases into the supplied tool cache; no game is touched.
use kryoto_repair::release;
#[tokio::main]
async fn main() {
    let cache = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .expect("Pass a dedicated test cache directory");
    let client = release::client().unwrap();
    for source in release::sources() {
        let tools = match release::install(&client, &cache, source.id, &|text, percent| {
            if percent.is_none() {
                println!("{text}");
            }
        })
        .await
        {
            Ok(tools) => tools,
            Err(error) => {
                eprintln!("FAILED {}: {error}", source.label);
                continue;
            }
        };
        println!(
            "OK {} {} SHA256 {}",
            source.label, tools.version, tools.archive_sha256
        );
        let cached = release::install(&client, &cache, source.id, &|_, _| {})
            .await
            .unwrap();
        assert_eq!(cached.archive_sha256, tools.archive_sha256);
    }
}
