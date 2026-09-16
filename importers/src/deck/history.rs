use agni_deck::Snapshot;
use spirit_core::collection::{self, Item};
use spirit_core::record::{Attestation, Cir, Claim, Tdr};
use spirit_core::{
    canonical, refs, BlobHash, BlobRef, BlobStore, CiHash, Dgid, Identity, TdHash, Trust,
    TrustLevel,
};

pub const DECK_KIND: &str = "deck";
pub const DECK_PREFIX: &str = "decks";

pub fn collection_name(game: &str) -> String {
    format!("{DECK_PREFIX}/{game}")
}

pub fn game_name(collection: &str) -> Option<&str> {
    collection
        .strip_prefix(DECK_PREFIX)?
        .strip_prefix('/')
        .filter(|game| refs::valid_segment(game))
}

#[derive(Debug, Clone, PartialEq)]
pub struct SavedDeck {
    pub ci: CiHash,
    pub label: String,
    pub blob: Option<BlobHash>,
    pub signer: Option<Dgid>,
    pub held: bool,
}

pub fn identity_of(snapshot: &Snapshot) -> Result<CiHash, String> {
    Cir::new(DECK_KIND, &snapshot.identity())
        .and_then(|cir| cir.address())
        .map_err(|e| e.to_string())
}

pub fn save(
    store: &BlobStore,
    identity: &Identity,
    snapshot: &Snapshot,
    label: &str,
    td: &Tdr,
) -> Result<CiHash, String> {
    if !refs::valid_segment(&snapshot.game) {
        return Err(format!("game {:?} is not a usable ref name", snapshot.game));
    }
    let cir = Cir::new(DECK_KIND, &snapshot.identity()).map_err(|e| e.to_string())?;
    let ci = store
        .put(&cir.encode().map_err(|e| e.to_string())?)
        .map(CiHash::from_hash)
        .map_err(|e| e.to_string())?;
    let blob = store
        .put(&canonical::to_vec(snapshot).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let td_hash = store
        .put(&td.encode().map_err(|e| e.to_string())?)
        .map(TdHash::from_hash)
        .map_err(|e| e.to_string())?;
    let attestation = Attestation::sign(
        Claim::content(ci, td_hash, BlobRef::from_hash(blob)),
        identity,
    )
    .map_err(|e| e.to_string())?;
    let attestation_hash = store
        .put(&attestation.encode().map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;

    let name = collection_name(&snapshot.game);
    let mut builder = collection::Builder::open(store, &name, identity);
    builder
        .add(Item::labelled(ci, label))
        .attest(attestation_hash)
        .record(ci.hash())
        .record(td_hash.hash())
        .record(blob);
    builder.publish(store, identity)?;
    Ok(ci)
}

fn claim_for(store: &BlobStore, attestations: &[BlobHash], ci: CiHash) -> Option<Attestation> {
    let claims: Vec<Attestation> = attestations
        .iter()
        .filter_map(|hash| Attestation::decode(&store.get(*hash).ok()?).ok())
        .filter(|attestation| attestation.claim.ci == ci)
        .collect();
    claims
        .iter()
        .rfind(|attestation| {
            attestation
                .claim
                .blob
                .is_some_and(|blob| store.has(blob.hash()))
        })
        .or_else(|| claims.last())
        .cloned()
}

pub fn list(store: &BlobStore, game: &str) -> Vec<SavedDeck> {
    let name = collection_name(game);
    let Some((head, ops)) = collection::load(store, &name) else {
        return Vec::new();
    };
    collection::fold(head.owner, &ops)
        .into_iter()
        .map(|item| {
            let attestation = claim_for(store, &head.attestations, item.ci);
            let blob = attestation
                .as_ref()
                .and_then(|found| found.claim.blob)
                .map(|blob| blob.hash());
            SavedDeck {
                ci: item.ci,
                label: item.label.unwrap_or_default(),
                blob,
                signer: attestation.as_ref().and_then(|found| found.signer()),
                held: blob.is_some_and(|hash| store.has(hash)),
            }
        })
        .collect()
}

pub fn load(store: &BlobStore, trust: &Trust, game: &str, ci: CiHash) -> Option<Snapshot> {
    let saved = list(store, game).into_iter().find(|deck| deck.ci == ci)?;
    if !saved
        .signer
        .is_some_and(|dgid| trust.trusts(dgid, TrustLevel::Cache))
    {
        return None;
    }
    canonical::from_slice(&store.get(saved.blob?).ok()?).ok()
}

pub fn forget(
    store: &BlobStore,
    identity: &Identity,
    game: &str,
    ci: CiHash,
) -> Result<(), String> {
    let name = collection_name(game);
    let mut builder = collection::Builder::open(store, &name, identity);
    builder.remove(ci);
    builder.publish(store, identity)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use agni_deck::{SnapshotCard, SnapshotZone};

    fn scratch(tag: &str) -> (BlobStore, Identity) {
        let dir =
            std::env::temp_dir().join(format!("agni-deck-history-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let store = BlobStore::open(&dir).unwrap();
        let identity = spirit_core::identity::load_or_create(&dir).unwrap();
        (store, identity)
    }

    fn td() -> Tdr {
        Tdr::new("kai-deck-import", &("paste",)).unwrap()
    }

    fn card(key: &str, count: u32, art: Option<&str>) -> SnapshotCard {
        SnapshotCard {
            count,
            image_url: art.map(String::from),
            kind: None,
            energy: None,
            power: None,
            might: None,
            key: key.into(),
            name: format!("Card {key}"),
            domain: Vec::new(),
            tags: Vec::new(),
            signature: false,
        }
    }

    fn deck(cards: Vec<SnapshotCard>) -> Snapshot {
        Snapshot::new("riftbound", vec![SnapshotZone::new("main", cards)])
    }

    fn trust_of(identity: &Identity) -> Trust {
        Trust::new().with_own(identity.dgid())
    }

    #[test]
    fn a_saved_deck_loads_back_whole() {
        let (store, me) = scratch("roundtrip");
        let snapshot = deck(vec![card("a", 2, Some("art")), card("b", 1, None)]);
        let ci = save(&store, &me, &snapshot, "Ashe", &td()).unwrap();

        let listed = list(&store, "riftbound");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].ci, ci);
        assert_eq!(listed[0].label, "Ashe");
        assert!(listed[0].held);
        assert_eq!(listed[0].signer, Some(me.dgid()));

        let loaded = load(&store, &trust_of(&me), "riftbound", ci).unwrap();
        assert_eq!(loaded, snapshot);
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn the_same_deck_saved_twice_is_one_entry() {
        let (store, me) = scratch("dedup");
        let snapshot = deck(vec![card("a", 2, None)]);
        let first = save(&store, &me, &snapshot, "Ashe", &td()).unwrap();
        let second = save(&store, &me, &snapshot, "Ashe", &td()).unwrap();
        assert_eq!(first, second);
        assert_eq!(list(&store, "riftbound").len(), 1);
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn re_importing_with_fresher_art_keeps_one_entry_and_takes_the_new_art() {
        let (store, me) = scratch("reimport");
        let bare = deck(vec![card("a", 2, None)]);
        let arted = deck(vec![card("a", 2, Some("https://art.example/a.png"))]);
        let first = save(&store, &me, &bare, "Ashe", &td()).unwrap();
        let second = save(&store, &me, &arted, "Ashe", &td()).unwrap();

        assert_eq!(first, second);
        assert_eq!(list(&store, "riftbound").len(), 1);
        let loaded = load(&store, &trust_of(&me), "riftbound", first).unwrap();
        assert_eq!(loaded, arted);
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn a_changed_list_is_a_second_deck() {
        let (store, me) = scratch("changed");
        let ashe = save(&store, &me, &deck(vec![card("a", 2, None)]), "Ashe", &td()).unwrap();
        let jinx = save(&store, &me, &deck(vec![card("a", 3, None)]), "Jinx", &td()).unwrap();
        assert_ne!(ashe, jinx);

        let listed = list(&store, "riftbound");
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].label, "Ashe");
        assert_eq!(listed[1].label, "Jinx");
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn a_relabel_renames_without_duplicating() {
        let (store, me) = scratch("relabel");
        let snapshot = deck(vec![card("a", 1, None)]);
        save(&store, &me, &snapshot, "first name", &td()).unwrap();
        save(&store, &me, &snapshot, "second name", &td()).unwrap();
        let listed = list(&store, "riftbound");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "second name");
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn each_game_keeps_its_own_history() {
        let (store, me) = scratch("games");
        save(&store, &me, &deck(vec![card("a", 1, None)]), "rb", &td()).unwrap();
        let mtg = Snapshot::new(
            "mtg",
            vec![SnapshotZone::new("main", vec![card("Shock", 4, None)])],
        );
        save(&store, &me, &mtg, "burn", &td()).unwrap();

        assert_eq!(list(&store, "riftbound").len(), 1);
        assert_eq!(list(&store, "mtg").len(), 1);
        assert!(list(&store, "nothing").is_empty());
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn a_forgotten_deck_leaves_the_list() {
        let (store, me) = scratch("forget");
        let ci = save(&store, &me, &deck(vec![card("a", 1, None)]), "Ashe", &td()).unwrap();
        save(&store, &me, &deck(vec![card("b", 1, None)]), "Jinx", &td()).unwrap();
        forget(&store, &me, "riftbound", ci).unwrap();
        let listed = list(&store, "riftbound");
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].label, "Jinx");
        assert!(load(&store, &trust_of(&me), "riftbound", ci).is_none());
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn an_untrusted_signers_deck_is_listed_but_never_loaded() {
        let (store, me) = scratch("untrusted");
        let stranger = Identity::from_secret([31; 32]);
        let ci = save(
            &store,
            &stranger,
            &deck(vec![card("a", 1, None)]),
            "theirs",
            &td(),
        )
        .unwrap();
        assert_eq!(list(&store, "riftbound").len(), 1);
        assert!(load(&store, &trust_of(&me), "riftbound", ci).is_none());

        let mut trust = trust_of(&me);
        trust.set(stranger.dgid(), TrustLevel::Cache);
        assert!(load(&store, &trust, "riftbound", ci).is_some());
        let _ = std::fs::remove_dir_all(store.root());
    }

    #[test]
    fn collection_names_round_trip() {
        assert_eq!(collection_name("mtg"), "decks/mtg");
        assert_eq!(game_name("decks/mtg"), Some("mtg"));
        assert_eq!(game_name("modules/mtg"), None);
    }
}
