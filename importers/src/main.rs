use std::path::PathBuf;

fn main() -> agni_importers::IngestResult<()> {
    let mut args = std::env::args().skip(1);
    let set = args
        .next()
        .ok_or("usage: ingest-scryfall <set> [store-dir]")?;
    let dir = args.next().map(PathBuf::from).unwrap_or_else(|| {
        let home = std::env::var("HOME").expect("HOME is not set");
        PathBuf::from(home).join(".spirit/store")
    });

    let ingested = agni_importers::ingest(&set, &dir, |done, total| {
        if done % 25 == 0 {
            println!("  {done}/{total}");
        }
    })?;

    println!(
        "manifest {} ({} cards, {} skipped) -> refs/{}",
        ingested.manifest, ingested.cards, ingested.skipped, set
    );
    Ok(())
}
