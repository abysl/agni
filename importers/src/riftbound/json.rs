use super::resolve::Resolution;
use agni_riftbound::{DeckEntry, ResolvedCard};
use serde_json::{json, Value};

fn card_json(card: &ResolvedCard) -> Value {
    json!({
        "name": card.name,
        "riftbound_id": card.riftbound_id,
        "image_url": card.image_url,
        "kind": card.kind,
        "energy": card.energy,
        "power": card.power,
        "might": card.might,
        "domain": card.domain,
        "tags": card.tags,
        "signature": card.signature,
    })
}

fn zone_json(entries: &[DeckEntry]) -> Value {
    Value::Array(
        entries
            .iter()
            .map(|entry| {
                let mut value = card_json(&entry.card);
                value["count"] = json!(entry.count);
                value
            })
            .collect(),
    )
}

pub fn deck_json(
    resolution: &Resolution,
    source_kind: &str,
    source_value: &str,
    code: Option<&str>,
) -> Value {
    let deck = &resolution.deck;
    json!({
        "game": "riftbound",
        "source": { "kind": source_kind, "value": source_value },
        "code": code,
        "deck": {
            "legend": deck.legend.as_ref().map(card_json),
            "chosen_champion": deck.chosen_champion.as_ref().map(card_json),
            "main_deck": zone_json(&deck.main_deck),
            "runes": zone_json(&deck.runes),
            "battlefields": zone_json(&deck.battlefields),
            "sideboard": zone_json(&deck.sideboard),
        },
        "unresolved": resolution.unresolved.iter().map(|entry| json!({
            "identifier": entry.identifier,
            "reason": entry.reason,
        })).collect::<Vec<Value>>(),
    })
}

#[cfg(test)]
mod tests {
    use super::super::catalog::test_catalog;
    use super::super::resolve::resolve;
    use super::super::{parse_deck, DeckSource};
    use super::*;

    #[test]
    fn the_normalized_json_names_every_zone_and_unresolved_card() {
        let text = "Legend\n1 Vanguard Sentinel\nDeck\n3 Emberwing Scout\n2 Nonexistent Wanderer\nRunes\n12 Ember Rune\nBattlefields\n1 Sunken Causeway\n";
        let parsed = parse_deck(&DeckSource::Text(text.into())).unwrap();
        let resolution = resolve(&parsed, &mut test_catalog()).unwrap();
        let value = deck_json(&resolution, "text", text, None);
        assert_eq!(value["game"], "riftbound");
        assert_eq!(value["source"]["kind"], "text");
        assert_eq!(value["deck"]["legend"]["name"], "Vanguard Sentinel");
        assert_eq!(value["deck"]["main_deck"][0]["count"], 3);
        assert_eq!(value["deck"]["main_deck"][0]["riftbound_id"], "ogn-007-298");
        assert_eq!(value["deck"]["runes"][0]["count"], 12);
        assert_eq!(value["deck"]["battlefields"][0]["name"], "Sunken Causeway");
        assert_eq!(value["deck"]["main_deck"][0]["tags"], json!([]));
        assert_eq!(value["deck"]["main_deck"][0]["signature"], false);
        assert_eq!(value["unresolved"][0]["identifier"], "Nonexistent Wanderer");
        assert!(value["deck"]["main_deck"][0]["image_url"]
            .as_str()
            .unwrap()
            .starts_with("https://img.example/"));
    }
}
