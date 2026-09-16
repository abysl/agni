use agni_importers::riftbound::ingest;
use std::path::{Path, PathBuf};

const USAGE: &str = "usage: ingest-riftbound [store-dir] [--audit] [--only <id>...] [--ids <file|->] [--pool <file.md>...]
  store-dir      the spirit blob store (default ~/.spirit/store); required when ids are given
  --audit        with ids: list the ids whose art the store lacks and write nothing
  --only ID ...  riftbound ids to fetch, up to the next flag
  --ids FILE     riftbound ids, one per line (# comments allowed); - reads stdin
  --pool FILE    a pool file whose card lines supply ids: - **Name** (id; Kind; Domain; cost): text
Without ids the whole Riftcodex set is ingested into the store.";

pub struct Args {
    pub store: Option<PathBuf>,
    pub audit: bool,
    pub ids: Vec<String>,
}

pub fn pool_line_id(line: &str) -> Option<&str> {
    let rest = line.trim_start().strip_prefix("- **")?;
    let (_, after_name) = rest.split_once("** (")?;
    let (id, _) = after_name.split_once(';')?;
    let id = id.trim();
    (!id.is_empty() && id.contains('-')).then_some(id)
}

pub fn ids_from_pool(text: &str) -> Vec<String> {
    text.lines()
        .filter_map(pool_line_id)
        .map(|id| id.to_ascii_lowercase())
        .collect()
}

pub fn ids_from_list(text: &str) -> Vec<String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|line| !line.is_empty())
        .map(|id| id.to_ascii_lowercase())
        .collect()
}

pub fn dedupe(ids: Vec<String>) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    ids.into_iter()
        .filter(|id| seen.insert(id.clone()))
        .collect()
}

fn read_input(path: &str) -> Result<String, String> {
    if path == "-" {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut std::io::stdin(), &mut text)
            .map_err(|error| format!("stdin: {error}"))?;
        return Ok(text);
    }
    std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))
}

pub fn parse_args(raw: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut store = None;
    let mut audit = false;
    let mut ids = Vec::new();
    let mut raw = raw.peekable();
    while let Some(arg) = raw.next() {
        match arg.as_str() {
            "--audit" => audit = true,
            "--only" => {
                while let Some(id) = raw.next_if(|next| !next.starts_with("--")) {
                    ids.extend(ids_from_list(&id));
                }
            }
            "--ids" => {
                let path = raw.next().ok_or("--ids needs a file or -")?;
                ids.extend(ids_from_list(&read_input(&path)?));
            }
            "--pool" => {
                let path = raw.next().ok_or("--pool needs a file")?;
                ids.extend(ids_from_pool(&read_input(&path)?));
            }
            "-h" | "--help" => return Err(USAGE.to_string()),
            other if other.starts_with("--") => {
                return Err(format!("unknown flag {other}\n{USAGE}"))
            }
            other if store.is_none() => store = Some(PathBuf::from(other)),
            other => return Err(format!("unexpected argument {other}\n{USAGE}")),
        }
    }
    if audit && ids.is_empty() {
        return Err(format!(
            "--audit needs ids: pass --only, --ids or --pool\n{USAGE}"
        ));
    }
    if !ids.is_empty() && store.is_none() {
        return Err(format!("name the store when ids are given\n{USAGE}"));
    }
    Ok(Args {
        store,
        audit,
        ids: dedupe(ids),
    })
}

fn ingest_everything(dir: &Path) -> agni_importers::IngestResult<bool> {
    let mut last_stage = "";
    let ingested = ingest::ingest(dir, |progress| {
        if progress.stage != last_stage {
            println!("{}:", progress.stage);
            last_stage = progress.stage;
        }
        if progress.done % 25 == 0 || progress.done == progress.total {
            println!("  {}/{}", progress.done, progress.total);
        }
    })?;
    println!(
        "manifest {} ({} cards, {} images fetched, {} reused, {} without art) -> refs/{}",
        ingested.manifest,
        ingested.cards,
        ingested.images_fetched,
        ingested.images_reused,
        ingested.skipped,
        ingest::REF_NAME
    );
    Ok(true)
}

fn ingest_some(dir: &Path, ids: &[String], audit: bool) -> agni_importers::IngestResult<bool> {
    let missing = ingest::audit(dir, ids)?;
    println!(
        "{}: {} ids wanted, {} without stored art",
        dir.display(),
        ids.len(),
        missing.len()
    );
    for id in &missing {
        println!("  missing {id}");
    }
    if audit || missing.is_empty() {
        return Ok(true);
    }
    println!(
        "fetching {} faces from riftcodex at one request per second ({} seconds at most)",
        missing.len(),
        missing.len() * 2
    );
    let outcome = ingest::ingest_ids(dir, &missing, |progress| {
        if progress.done % 10 == 0 || progress.done == progress.total {
            println!("  {}/{}", progress.done, progress.total);
        }
    })?;
    for id in &outcome.fetched {
        println!("  fetched {id}");
    }
    for id in &outcome.reused {
        println!("  reused {id}");
    }
    for id in &outcome.unknown {
        println!("  unknown to riftcodex: {id}");
    }
    for id in &outcome.artless {
        println!("  riftcodex has no image for {id}");
    }
    match outcome.manifest {
        Some(hash) => println!("manifest {hash} -> refs/{}", ingest::REF_NAME),
        None => println!("manifest unchanged"),
    }
    println!(
        "{} fetched, {} reused, {} unknown, {} artless",
        outcome.fetched.len(),
        outcome.reused.len(),
        outcome.unknown.len(),
        outcome.artless.len()
    );
    let still = ingest::audit(dir, ids)?;
    for id in &still {
        println!("  still missing {id}");
    }
    Ok(still.is_empty())
}

fn main() {
    let args = match parse_args(std::env::args().skip(1)) {
        Ok(args) => args,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };
    let dir = args.store.clone().unwrap_or_else(|| {
        let home = std::env::var("HOME").expect("HOME is not set");
        PathBuf::from(home).join(".spirit/store")
    });
    let outcome = if args.ids.is_empty() {
        ingest_everything(&dir)
    } else {
        ingest_some(&dir, &args.ids, args.audit)
    };
    match outcome {
        Ok(true) => {}
        Ok(false) => std::process::exit(1),
        Err(error) => {
            eprintln!("ingest failed: {error} — running it again resumes where it stopped");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pool_lines_yield_their_ids_and_nothing_else() {
        let text = "# Master Yi (Akame)\n\n- **Master Yi - Wuju Bladesman** (ogs-019-024; Legend; Calm/Body; ): While a friendly unit defends alone, it gets +2.\n- **Blade Dance** (UNL-047-219; Spell; Body; 2): text\nnot a card line\n- **Broken** (): nothing\n";
        assert_eq!(
            ids_from_pool(text),
            vec!["ogs-019-024".to_string(), "unl-047-219".to_string()]
        );
    }

    #[test]
    fn id_lists_skip_comments_and_blank_lines() {
        let text = "ven-192-166 # Nasus\n\n# whole comment\nOGS-019-024\n";
        assert_eq!(
            ids_from_list(text),
            vec!["ven-192-166".to_string(), "ogs-019-024".to_string()]
        );
    }

    #[test]
    fn args_default_to_the_whole_set_and_need_a_store_for_ids() {
        let whole = parse_args(std::iter::empty()).unwrap();
        assert!(whole.store.is_none() && whole.ids.is_empty() && !whole.audit);
        let named = parse_args(["/tmp/store".to_string()].into_iter()).unwrap();
        assert_eq!(named.store, Some(PathBuf::from("/tmp/store")));
        assert!(parse_args(["--audit".to_string()].into_iter()).is_err());
        assert!(parse_args(["--only".to_string(), "ven-192-166".to_string()].into_iter()).is_err());
        let dir = std::env::temp_dir().join(format!("ingest-riftbound-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let list = dir.join("ids.txt");
        std::fs::write(&list, "ven-192-166\nven-192-166\nogs-019-024\n").unwrap();
        let pool = dir.join("pool.md");
        std::fs::write(
            &pool,
            "- **Blade Dance** (UNL-047-219; Spell; Body; 2): text\n",
        )
        .unwrap();
        let args = parse_args(
            [
                "/tmp/store".to_string(),
                "--audit".to_string(),
                "--only".to_string(),
                "OGN-001-001".to_string(),
                "ven-192-166".to_string(),
                "--ids".to_string(),
                list.display().to_string(),
                "--pool".to_string(),
                pool.display().to_string(),
            ]
            .into_iter(),
        )
        .unwrap();
        assert!(args.audit);
        assert_eq!(args.store, Some(PathBuf::from("/tmp/store")));
        assert_eq!(
            args.ids,
            vec![
                "ogn-001-001".to_string(),
                "ven-192-166".to_string(),
                "ogs-019-024".to_string(),
                "unl-047-219".to_string()
            ]
        );
        assert!(parse_args(["/a".to_string(), "/b".to_string()].into_iter()).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
