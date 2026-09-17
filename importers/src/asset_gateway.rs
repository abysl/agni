use crate::art::{self, Journal};
use serde_json::json;
use spirit_core::{BlobHash, BlobStore};
use spirit_node::assets::{self, Found};
use spirit_node::gateway::{ResolveReply, ResolveRequest, Resolver};
use spirit_node::mesh::Mesh;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const RESOLVER_NAME: &str = "asset";

pub fn publish_index(dir: &Path) -> Result<BlobHash, String> {
    let store = BlobStore::open(dir).map_err(|error| error.to_string())?;
    let mut entries = art::asset_entries(&store);
    #[cfg(feature = "riftbound-native")]
    if let Ok(Some(manifest)) = crate::riftbound::ingest::load_manifest(dir) {
        for card in &manifest.cards {
            if let Some(hash) = BlobHash::parse(&card.image).filter(|hash| store.has(*hash)) {
                entries.insert(
                    art::asset_key(crate::riftbound::ingest::JOURNAL_FILE, &card.riftbound_id),
                    hash,
                );
            }
        }
    }
    assets::publish(&store, &entries)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Held(BlobHash),
    Pending(&'static str),
    Fetch,
}

pub fn decide(store: &BlobStore, journal: &str, name: &str, key: &str, mesh: &Mesh) -> Outcome {
    let mut local = Journal::open(store, journal);
    if let Some(hash) = local.get(store, name) {
        return Outcome::Held(hash);
    }
    match mesh.find_asset(store, key) {
        Found::Held(hash) => {
            let _ = local.link(name, hash);
            Outcome::Held(hash)
        }
        Found::Provider(hash, peer) => {
            mesh.request_blob(hash, Some(&peer));
            Outcome::Pending("fetching the blob from a peer")
        }
        Found::Unknown => {
            let missing = mesh.missing_asset_indexes(store);
            if missing.is_empty() {
                return Outcome::Fetch;
            }
            for (peer, manifest) in missing {
                mesh.request_blob(manifest, Some(&peer));
            }
            Outcome::Pending("asking peers for their asset indexes")
        }
    }
}

pub fn asset_resolver(dir: PathBuf, mesh: Arc<Mesh>) -> Resolver {
    resolver(dir, Some(mesh))
}

pub fn published_asset_resolver(dir: PathBuf) -> Resolver {
    resolver(dir, None)
}

fn resolver(dir: PathBuf, mesh: Option<Arc<Mesh>>) -> Resolver {
    Arc::new(move |request: &ResolveRequest| {
        let Some(key) = request
            .params
            .get("key")
            .map(|key| key.trim().to_ascii_lowercase())
        else {
            return ResolveReply::error(400, "an asset request names its key");
        };
        let Some((journal, name)) = art::split_asset_key(&key) else {
            return ResolveReply::error(
                400,
                "an asset key is <journal>/<name> over a known journal",
            );
        };
        let Ok(store) = BlobStore::open(&dir) else {
            return ResolveReply::error(500, "store unavailable");
        };
        let outcome = match &mesh {
            Some(mesh) => decide(&store, journal, name, &key, mesh),
            None => match Journal::open(&store, journal).get(&store, name) {
                Some(hash) => Outcome::Held(hash),
                None => return ResolveReply::error(404, "asset not published by this service"),
            },
        };
        match outcome {
            Outcome::Held(hash) => {
                let _ = publish_index(&dir);
                ResolveReply::json(200, json!({ "hash": hash.to_string() }).to_string())
            }
            Outcome::Pending(why) => ResolveReply::json(202, json!({ "pending": why }).to_string()),
            Outcome::Fetch => {
                let Some(url) = request.params.get("url").map(|url| url.trim()) else {
                    return ResolveReply::error(
                        404,
                        "no node holds that asset and no url was given",
                    );
                };
                if !art::asset_url_allowed(url) {
                    return ResolveReply::error(403, "that host is not an allowed asset source");
                }
                let agent = art::art_agent();
                match art::fetch_one(&store, journal, &agent, name, url) {
                    Ok((hash, _)) => {
                        let _ = publish_index(&dir);
                        ResolveReply::json(200, json!({ "hash": hash.to_string() }).to_string())
                    }
                    Err(error) => ResolveReply::error(502, &format!("{url}: {error}")),
                }
            }
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn published_service_refuses_unknown_assets_even_with_an_allowed_source_url() {
        let dir =
            std::env::temp_dir().join(format!("agni-published-assets-{}", std::process::id()));
        std::fs::create_dir(&dir).unwrap();
        let store = BlobStore::open(&dir).unwrap();
        let hash = Journal::open(&store, "playmats")
            .put(&store, "approved", b"published image")
            .unwrap();
        let resolve = published_asset_resolver(dir.clone());
        let mut request = ResolveRequest {
            name: RESOLVER_NAME.into(),
            params: BTreeMap::from([
                ("key".into(), "playmats/unknown".into()),
                ("url".into(), "https://cards.scryfall.io/random.jpg".into()),
            ]),
        };
        assert_eq!(resolve(&request).status, 404);
        request
            .params
            .insert("key".into(), "playmats/approved".into());
        let reply = resolve(&request);
        assert_eq!(reply.status, 200);
        assert!(String::from_utf8(reply.body)
            .unwrap()
            .contains(&hash.to_string()));
        assert_eq!(art::load_journal(&store, "playmats").len(), 1);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
