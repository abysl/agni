use agni_importers::art::{self, Journal};
use spirit_core::{envelope, refs, BlobHash, BlobStore};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

fn copy_closure(
    source: &BlobStore,
    destination: &BlobStore,
    roots: &[BlobHash],
) -> Result<usize, String> {
    let mut pending = roots.to_vec();
    let mut copied = BTreeSet::new();
    while let Some(hash) = pending.pop() {
        if copied.contains(&hash) || !source.has(hash) {
            continue;
        }
        let bytes = source.get(hash).map_err(|e| e.to_string())?;
        if let Some(record) = envelope::of(&bytes) {
            pending.extend(record.refs);
        }
        let actual = destination.put(&bytes).map_err(|e| e.to_string())?;
        if actual != hash {
            return Err("Source fingerprint mismatch".into());
        }
        copied.insert(hash);
    }
    Ok(copied.len())
}

fn curate(
    source: &Path,
    destination: &Path,
    approved: &BTreeMap<String, BlobHash>,
    assets: &BTreeMap<String, BlobHash>,
) -> Result<usize, String> {
    if !source.is_dir() {
        return Err("Source store does not exist".into());
    }
    let source = BlobStore::open(source).map_err(|e| e.to_string())?;
    for (name, hash) in approved {
        if refs::read(&source, name) != Some(*hash) || !source.has(*hash) {
            return Err(format!("Approved ref {name} is absent or has changed"));
        }
    }
    for (key, hash) in assets {
        if art::split_asset_key(key).is_none() || !source.has(*hash) || key.contains(['\n', '\r']) {
            return Err(format!("Invalid or missing approved asset {key}"));
        }
    }
    std::fs::create_dir(destination)
        .map_err(|e| format!("Destination must be a new directory: {e}"))?;
    let destination = BlobStore::open(destination).map_err(|e| e.to_string())?;
    let copied = copy_closure(
        &source,
        &destination,
        &approved.values().copied().collect::<Vec<_>>(),
    )?;
    for (name, hash) in approved {
        refs::write(&destination, name, *hash)?;
    }
    for journal in ["riftbound-images", "mtg-images"] {
        let mut output = Journal::open(&destination, journal);
        for (key, hash) in art::load_journal(&source, journal) {
            if destination.has(hash) {
                output.link(&key, hash).map_err(|e| e.to_string())?;
            }
        }
    }
    for (key, hash) in assets {
        let (journal, name) = art::split_asset_key(key).ok_or("Invalid asset key")?;
        let bytes = source.get(*hash).map_err(|e| e.to_string())?;
        Journal::open(&destination, journal)
            .put(&destination, name, &bytes)
            .map_err(|e| e.to_string())?;
    }
    agni_importers::asset_gateway::publish_index(destination.root())?;
    Ok(copied)
}

fn pair(value: &str) -> Result<(String, BlobHash), String> {
    let (key, hash) = value.rsplit_once('=').ok_or("Expected name=hash")?;
    Ok((
        key.into(),
        BlobHash::parse(hash).ok_or("Invalid content hash")?,
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args().skip(1);
    let source = args
        .next()
        .ok_or("Usage: curate-store SOURCE NEW_DEST --ref name=hash --asset journal/name=hash")?;
    let destination = args.next().ok_or("Expected a new destination directory")?;
    let mut approved = BTreeMap::new();
    let mut assets = BTreeMap::new();
    while let Some(option) = args.next() {
        let (name, hash) = pair(&args.next().ok_or("Expected name=hash")?)?;
        match option.as_str() {
            "--ref" => {
                approved.insert(name, hash);
            }
            "--asset" => {
                assets.insert(name, hash);
            }
            _ => return Err(format!("Unknown option {option}").into()),
        }
    }
    if approved.is_empty() && assets.is_empty() {
        return Err("At least one approved ref or asset is required".into());
    }
    let copied = curate(
        Path::new(&source),
        Path::new(&destination),
        &approved,
        &assets,
    )?;
    println!(
        "Copied {copied} reachable blobs and {} explicit assets; source untouched",
        assets.len()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_selected_closures_and_explicit_assets_leave_the_source() {
        let root = std::env::temp_dir().join(format!("agni-curation-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let source = BlobStore::open(root.join("source")).unwrap();
        let card = source.put(b"approved card").unwrap();
        let random = source.put(b"unrelated personal image").unwrap();
        let manifest = source
            .put(
                &spirit_core::canonical::to_vec(&envelope::Envelope {
                    kind: "test".into(),
                    refs: vec![card],
                })
                .unwrap(),
            )
            .unwrap();
        refs::write(&source, "cards", manifest).unwrap();
        Journal::open(&source, "playmats")
            .link("random", random)
            .unwrap();
        let approved = BTreeMap::from([("cards".into(), manifest)]);
        let destination = root.join("published");
        assert_eq!(
            curate(source.root(), &destination, &approved, &BTreeMap::new()).unwrap(),
            2
        );
        let held = BlobStore::open(&destination).unwrap();
        assert!(held.has(card));
        assert!(!held.has(random));
        assert!(source.has(random));
        assert!(curate(source.root(), &destination, &approved, &BTreeMap::new()).is_err());
        std::fs::remove_dir_all(root).unwrap();
    }
}
