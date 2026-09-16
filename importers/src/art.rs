use crate::IngestResult;
use spirit_core::{BlobHash, BlobStore};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::Duration;

pub const USER_AGENT: &str = "agni-importers/0.1 (personal card store; contact dev@rae.blue)";

pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub stage: &'static str,
}

pub struct WantedArt {
    pub key: String,
    pub name: String,
    pub image_url: Option<String>,
}

pub struct FetchedArt {
    pub key: String,
    pub hash: BlobHash,
    pub bytes: Vec<u8>,
}

pub fn art_agent() -> ureq::Agent {
    ureq::AgentBuilder::new().user_agent(USER_AGENT).build()
}

pub fn fetch_image(agent: &ureq::Agent, url: &str) -> IngestResult<Vec<u8>> {
    let mut bytes = Vec::new();
    agent
        .get(url)
        .call()?
        .into_reader()
        .read_to_end(&mut bytes)?;
    Ok(bytes)
}

pub fn load_journal(store: &BlobStore, journal_file: &str) -> BTreeMap<String, BlobHash> {
    let mut journal = BTreeMap::new();
    let Ok(text) = std::fs::read_to_string(store.root().join(journal_file)) else {
        return journal;
    };
    for line in text.lines() {
        if let Some((key, hash)) = line.trim().rsplit_once(' ') {
            if let Some(hash) = BlobHash::parse(hash) {
                journal.insert(key.to_string(), hash);
            }
        }
    }
    journal
}

pub struct Journal {
    path: PathBuf,
    entries: BTreeMap<String, BlobHash>,
}

impl Journal {
    pub fn open(store: &BlobStore, journal_file: &str) -> Self {
        let entries = load_journal(store, journal_file)
            .into_iter()
            .map(|(key, hash)| (key.to_ascii_lowercase(), hash))
            .collect();
        Self {
            path: store.root().join(journal_file),
            entries,
        }
    }

    pub fn get(&self, store: &BlobStore, key: &str) -> Option<BlobHash> {
        self.entries
            .get(&key.to_ascii_lowercase())
            .copied()
            .filter(|hash| store.has(*hash))
    }

    pub fn put(&mut self, store: &BlobStore, key: &str, bytes: &[u8]) -> IngestResult<BlobHash> {
        let hash = store.put(bytes)?;
        self.link(key, hash)?;
        Ok(hash)
    }

    pub fn link(&mut self, key: &str, hash: BlobHash) -> IngestResult<()> {
        let key = key.to_ascii_lowercase();
        if self.entries.get(&key) == Some(&hash) {
            return Ok(());
        }
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{key} {hash}")?;
        file.flush()?;
        self.entries.insert(key, hash);
        Ok(())
    }

    pub fn entries(&self) -> &BTreeMap<String, BlobHash> {
        &self.entries
    }
}

pub const ASSET_JOURNALS: [&str; 4] = ["playmats", "card-backs", "riftbound-images", "mtg-images"];

pub const ASSET_HOSTS: [&str; 4] = [
    "cmsassets.rgpub.io",
    "cards.scryfall.io",
    "c1.scryfall.com",
    "c2.scryfall.com",
];

pub fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://").map(|(_, rest)| rest).unwrap_or(url);
    let host = rest.split(['/', '?', '#']).next()?;
    let host = host.rsplit('@').next()?;
    let host = host.split(':').next()?.trim().to_ascii_lowercase();
    (!host.is_empty()).then_some(host)
}

pub fn asset_url_allowed(url: &str) -> bool {
    url.starts_with("https://")
        && host_of(url).is_some_and(|host| ASSET_HOSTS.contains(&host.as_str()))
}

pub fn asset_key(journal: &str, key: &str) -> String {
    format!("{journal}/{}", key.trim().to_ascii_lowercase())
}

pub fn split_asset_key(key: &str) -> Option<(&str, &str)> {
    let (journal, name) = key.split_once('/')?;
    (ASSET_JOURNALS.contains(&journal) && !name.is_empty()).then_some((journal, name))
}

pub fn asset_entries(store: &BlobStore) -> BTreeMap<String, BlobHash> {
    let mut entries = BTreeMap::new();
    for journal in ASSET_JOURNALS {
        for (key, hash) in load_journal(store, journal) {
            if store.has(hash) {
                entries.insert(asset_key(journal, &key), hash);
            }
        }
    }
    entries
}

pub fn fetch_one(
    store: &BlobStore,
    journal_file: &str,
    agent: &ureq::Agent,
    key: &str,
    url: &str,
) -> IngestResult<(BlobHash, Vec<u8>)> {
    let mut journal = Journal::open(store, journal_file);
    if let Some(hash) = journal.get(store, key) {
        let bytes = store.get(hash)?;
        return Ok((hash, bytes));
    }
    let bytes = fetch_image(agent, url)?;
    let hash = journal.put(store, key, &bytes)?;
    Ok((hash, bytes))
}

pub fn fetch_deck_art(
    store: &BlobStore,
    journal_file: &str,
    throttle: Duration,
    wanted: &[WantedArt],
    known: impl Fn(&str) -> Option<BlobHash>,
    mut progress: impl FnMut(Progress),
) -> IngestResult<Vec<FetchedArt>> {
    let mut journal = Journal::open(store, journal_file);
    let agent = art_agent();
    let mut arts = Vec::new();
    let total = wanted.len();
    for (index, want) in wanted.iter().enumerate() {
        let key = want.key.to_ascii_lowercase();
        let stored = known(&key).filter(|hash| store.has(*hash));
        let hash = match stored.or_else(|| journal.get(store, &key)) {
            Some(hash) => Some(hash),
            None => match want.image_url.as_deref() {
                None | Some("") => None,
                Some(url) => {
                    std::thread::sleep(throttle);
                    let bytes = fetch_image(&agent, url)?;
                    Some(journal.put(store, &key, &bytes)?)
                }
            },
        };
        if let Some(hash) = hash {
            arts.push(FetchedArt {
                key: want.key.clone(),
                hash,
                bytes: store.get(hash)?,
            });
        }
        progress(Progress {
            done: index + 1,
            total,
            stage: "images",
        });
    }
    Ok(arts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_keys_name_their_journal_and_only_allowlisted_hosts_fetch() {
        assert_eq!(
            asset_key("playmats", " Playmat:Akali "),
            "playmats/playmat:akali"
        );
        assert_eq!(
            split_asset_key("riftbound-images/ogn-001-298"),
            Some(("riftbound-images", "ogn-001-298"))
        );
        assert_eq!(split_asset_key("secrets/x"), None);
        assert_eq!(split_asset_key("playmats/"), None);
        assert!(asset_url_allowed(
            "https://cmsassets.rgpub.io/sanity/images/a.jpg?w=1"
        ));
        assert!(asset_url_allowed(
            "https://cards.scryfall.io/large/front/a.jpg"
        ));
        assert!(
            !asset_url_allowed("http://cmsassets.rgpub.io/a.jpg"),
            "https only"
        );
        assert!(!asset_url_allowed(
            "https://cmsassets.rgpub.io.evil.com/a.jpg"
        ));
        assert!(!asset_url_allowed(
            "https://user@evil.com/?u=cards.scryfall.io"
        ));
        assert!(!asset_url_allowed("https://example.org/mine.png"));
    }

    #[test]
    fn a_journal_links_held_blobs_and_the_asset_entries_union_every_journal() {
        let dir = std::env::temp_dir().join(format!("agni-assets-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let store = BlobStore::open(&dir).unwrap();
        let mut mats = Journal::open(&store, "playmats");
        let mat = mats.put(&store, "playmat:Akali", b"mat bytes").unwrap();
        let mut backs = Journal::open(&store, "card-backs");
        let back = store.put(b"back bytes").unwrap();
        backs.link("back:riftbound", back).unwrap();
        backs.link("back:riftbound", back).unwrap();
        assert_eq!(backs.entries().len(), 1);
        let lines = std::fs::read_to_string(dir.join("card-backs")).unwrap();
        assert_eq!(
            lines.lines().count(),
            1,
            "a repeated link writes no second line"
        );
        let ghost = BlobHash::of(b"never stored");
        backs.link("back:ghost", ghost).unwrap();
        let entries = asset_entries(&store);
        assert_eq!(entries.get("playmats/playmat:akali"), Some(&mat));
        assert_eq!(entries.get("card-backs/back:riftbound"), Some(&back));
        assert!(
            !entries.contains_key("card-backs/back:ghost"),
            "only held blobs are advertised"
        );
        assert_eq!(
            Journal::open(&store, "playmats").get(&store, "PLAYMAT:AKALI"),
            Some(mat)
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agni-art-core-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn journal_keys_keep_their_spaces() {
        let dir = scratch("spaced");
        let store = BlobStore::open(&dir).unwrap();
        let hash = store.put(b"jpeg bytes").unwrap();
        std::fs::write(
            store.root().join("test-images"),
            format!("thornspire adept {hash}\nbroken line\n"),
        )
        .unwrap();
        let journal = load_journal(&store, "test-images");
        assert_eq!(journal.len(), 1);
        assert_eq!(journal.get("thornspire adept"), Some(&hash));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn journaled_art_is_reused_without_a_fetch() {
        let dir = scratch("reuse");
        let store = BlobStore::open(&dir).unwrap();
        let art = b"jpeg bytes for the adept".to_vec();
        let hash = store.put(&art).unwrap();
        std::fs::write(
            store.root().join("test-images"),
            format!("Thornspire Adept {hash}\n"),
        )
        .unwrap();
        let wanted = vec![
            WantedArt {
                key: "thornspire adept".into(),
                name: "Thornspire Adept".into(),
                image_url: Some("https://img.example/adept.jpg".into()),
            },
            WantedArt {
                key: "unfetchable".into(),
                name: "Unfetchable".into(),
                image_url: None,
            },
        ];
        let mut seen = Vec::new();
        let arts = fetch_deck_art(
            &store,
            "test-images",
            Duration::from_millis(0),
            &wanted,
            |_| None,
            |progress| seen.push((progress.done, progress.total)),
        )
        .unwrap();
        assert_eq!(seen, vec![(1, 2), (2, 2)]);
        assert_eq!(arts.len(), 1);
        assert_eq!(arts[0].key, "thornspire adept");
        assert_eq!(arts[0].bytes, art);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_journal_round_trips_a_single_card_without_the_network() {
        let dir = scratch("single");
        let store = BlobStore::open(&dir).unwrap();
        let mut journal = Journal::open(&store, "test-images");
        assert!(journal.get(&store, "Thornspire Adept").is_none());
        let hash = journal
            .put(&store, "Thornspire Adept", b"fresh art bytes")
            .unwrap();
        assert_eq!(journal.get(&store, "thornspire adept"), Some(hash));
        let reopened = Journal::open(&store, "test-images");
        assert_eq!(reopened.get(&store, "THORNSPIRE ADEPT"), Some(hash));
        assert_eq!(store.get(hash).unwrap(), b"fresh art bytes");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
