pub const PLUGIN_ABI_VERSION: u32 = agni_plugin_sdk::PLUGIN_ABI_VERSION;

pub const MANIFEST: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/manifest.cbor"));
pub const ACCEPT: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/accept.cbor"));
pub const VIEW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/view.cbor"));

pub fn decide_bytes(request: &[u8]) -> Vec<u8> {
    agni_riftbound_turns::decide_bytes(request)
}

pub fn view_bytes(request: &[u8]) -> Vec<u8> {
    agni_riftbound_turns::present::present_bytes(request)
}

agni_plugin_sdk::export_plugin!(
    manifest: crate::MANIFEST,
    decide: crate::decide_bytes,
    view: crate::view_bytes,
);

#[cfg(test)]
mod tests {
    use super::*;
    use agni_riftbound_turns::{GameBlob, Mode, Refusal, TurnEvent};
    use agni_sim::abi::DecideRequest;
    use agni_sim::log::{LogAction, LogEntry, LogState, Verdict};
    use agni_sim::wire::decode_plugin_manifest;
    use serde_bytes::ByteBuf;

    #[test]
    fn the_manifest_declares_the_riftbound_zone_table() {
        let manifest = decode_plugin_manifest(MANIFEST).unwrap();
        assert_eq!(manifest.name, "riftbound");
        assert_eq!(manifest.display, "Riftbound");
        assert_eq!(manifest.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(manifest.zones, agni_riftbound::zone_table());
        let keys: Vec<&str> = manifest
            .hotkeys
            .iter()
            .map(|hotkey| hotkey.key.as_str())
            .collect();
        assert_eq!(keys, ["e", "d", "t", "space", "w", "x", "h", "r", "k"]);
        assert_eq!(manifest.tokens, agni_riftbound::token_table());
        assert_eq!(manifest.tokens[0].face().might, Some(3));
    }

    #[test]
    fn the_table_options_kai_writes_are_the_ones_the_plugin_reads() {
        use agni_riftbound_turns::rules;
        assert_eq!(
            agni_riftbound::OPTION_VICTORY_SCORE,
            rules::OPTION_VICTORY_SCORE
        );
        assert_eq!(
            agni_riftbound::OPTION_BATTLEFIELDS,
            rules::OPTION_BATTLEFIELDS
        );
        assert_eq!(
            agni_riftbound::OPTION_RULES_ENFORCED,
            rules::OPTION_RULES_ENFORCED
        );
        let defaults = agni_riftbound::TableOptions::default();
        assert_eq!(defaults.victory_score, rules::DEFAULT_VICTORY_SCORE);
        assert_eq!(
            usize::from(defaults.battlefields),
            rules::DEFAULT_BATTLEFIELDS
        );
        let chosen = agni_riftbound::TableOptions {
            victory_score: 6,
            battlefields: 3,
        };
        let mut state = seated_state();
        state.options = Some(ByteBuf::from(chosen.encode()));
        let snapshot = agni_plugin_sdk::decide::parse(&request(
            &state,
            LogEntry::new(
                0,
                0,
                LogAction::Game {
                    data: ByteBuf::new(),
                },
            ),
        ))
        .unwrap()
        .table;
        let read = rules::Options::of(&snapshot);
        assert_eq!((read.victory_score, read.battlefields), (6, 3));
        assert!(!read.enforced);
        for (chosen, enforced) in [(None, true), (Some(chosen), true), (Some(chosen), false)] {
            let mut state = seated_state();
            let genesis = agni_riftbound::TableOptions::genesis(chosen, enforced).expect("written");
            state.options = Some(ByteBuf::from(genesis.clone()));
            let snapshot = agni_plugin_sdk::decide::parse(&request(
                &state,
                LogEntry::new(
                    0,
                    0,
                    LogAction::Game {
                        data: ByteBuf::new(),
                    },
                ),
            ))
            .unwrap()
            .table;
            let read = rules::Options::of(&snapshot);
            assert_eq!(read.enforced, enforced, "{chosen:?} {enforced}");
            assert_eq!(
                read.starting_mode(),
                if enforced { Mode::Enforced } else { Mode::Free }
            );
            assert_eq!(
                agni_riftbound::TableOptions::enforced_in(Some(&genesis)),
                enforced
            );
            assert_eq!(
                read.victory_score,
                chosen.map_or(rules::DEFAULT_VICTORY_SCORE, |options| options
                    .victory_score)
            );
        }
        let mut zeros: std::collections::BTreeMap<&str, i64> = std::collections::BTreeMap::new();
        zeros.insert(rules::OPTION_VICTORY_SCORE, 0);
        zeros.insert(rules::OPTION_BATTLEFIELDS, 0);
        let mut wild = zeros.clone();
        wild.insert(rules::OPTION_VICTORY_SCORE, 30);
        wild.insert(rules::OPTION_BATTLEFIELDS, 200);
        for bytes in [
            None,
            Some(agni_sim::abi::encode(&zeros)),
            Some(agni_sim::abi::encode(&wild)),
            Some(chosen.encode()),
        ] {
            let mut state = seated_state();
            state.options = bytes.clone().map(ByteBuf::from);
            let snapshot = agni_plugin_sdk::decide::parse(&request(
                &state,
                LogEntry::new(
                    0,
                    0,
                    LogAction::Game {
                        data: ByteBuf::new(),
                    },
                ),
            ))
            .unwrap()
            .table;
            let plugin = rules::Options::of(&snapshot);
            let kai = agni_riftbound::TableOptions::in_play(bytes.as_deref(), snapshot.players);
            assert_eq!(
                (
                    plugin.victory_score,
                    plugin.battlefields.min(agni_riftbound::BATTLEFIELD_COUNT)
                ),
                (kai.victory_score, usize::from(kai.battlefields)),
                "kai's fallback is the plugin's for {bytes:?}"
            );
        }
    }

    #[test]
    fn the_baked_accept_verdict_still_decodes() {
        let verdict: Verdict = agni_sim::abi::decode(ACCEPT).unwrap();
        assert!(verdict.accept);
        assert!(verdict.plugin_state.is_none());
    }

    fn request(state: &LogState, entry: LogEntry) -> Vec<u8> {
        agni_sim::abi::encode(&DecideRequest {
            plugin_state: state.plugin_state.clone(),
            state: state.clone(),
            entry,
        })
    }

    fn seated_state() -> LogState {
        LogState {
            zones: agni_riftbound::zone_table(),
            seats: (0..2u8)
                .map(|seat| agni_sim::log::Seat {
                    seat,
                    name: format!("seat {seat}"),
                })
                .collect(),
            ..LogState::default()
        }
    }

    #[test]
    fn the_decider_reads_the_engines_real_request_bytes() {
        let state = seated_state();
        let commit = request(
            &state,
            LogEntry::new(
                0,
                1,
                LogAction::Game {
                    data: ByteBuf::from(
                        TurnEvent::CommitRoll {
                            commit: agni_plugin_sdk::dice::commitment(&[5; 8]),
                        }
                        .encode(),
                    ),
                },
            ),
        );
        let verdict: Verdict = agni_sim::abi::decode(&decide_bytes(&commit)).unwrap();
        assert!(verdict.accept);
        let lobby = GameBlob::decode(verdict.plugin_state.as_deref().unwrap()).unwrap();
        assert!(lobby.roll().unwrap().hands[1].commit.is_some());
        let opened = GameBlob::start(2, 1, Mode::Free);

        let mut mid = state.clone();
        mid.plugin_state = ByteBuf::from(opened.encode());
        let wrong_seat = request(
            &mid,
            LogEntry::new(
                1,
                0,
                LogAction::Game {
                    data: ByteBuf::from(TurnEvent::EndTurn.encode()),
                },
            ),
        );
        let refused: Verdict = agni_sim::abi::decode(&decide_bytes(&wrong_seat)).unwrap();
        assert!(!refused.accept);
        assert_eq!(refused.reason, Some(Refusal::NotYourTurn.label()));
        assert_eq!(
            refused.refusal(),
            agni_sim::log::FoldError::Rejected {
                reason: Some("it is not your turn".into())
            }
        );
        let retired = request(
            &mid,
            LogEntry::new(
                1,
                1,
                LogAction::Game {
                    data: ByteBuf::from(vec![1]),
                },
            ),
        );
        let unreadable: Verdict = agni_sim::abi::decode(&decide_bytes(&retired)).unwrap();
        assert_eq!(unreadable.reason, Some(Refusal::BadEvent.label()));
        for event in [
            TurnEvent::Pass,
            TurnEvent::Pick(agni_plugin_sdk::prompt::Pick {
                prompt: 0,
                option: 0,
            }),
        ] {
            let bytes = request(
                &mid,
                LogEntry::new(
                    1,
                    1,
                    LogAction::Game {
                        data: ByteBuf::from(event.encode()),
                    },
                ),
            );
            let refused: Verdict = agni_sim::abi::decode(&decide_bytes(&bytes)).unwrap();
            assert!(!refused.accept, "{event:?}");
            assert!(refused.reason.is_some(), "{event:?}");
        }
        let ignored = request(
            &mid,
            LogEntry::new(
                1,
                1,
                LogAction::Game {
                    data: ByteBuf::from(
                        TurnEvent::Activate {
                            source: 3,
                            ability: 0,
                        }
                        .encode(),
                    ),
                },
            ),
        );
        let accepted: Verdict = agni_sim::abi::decode(&decide_bytes(&ignored)).unwrap();
        assert!(accepted.accept);
        assert_eq!(
            GameBlob::decode(accepted.plugin_state.as_deref().unwrap()),
            Some(opened.clone())
        );

        let a_move = request(
            &mid,
            LogEntry::new(
                1,
                0,
                LogAction::Move {
                    card: 3,
                    to: agni_core::Zone::Board,
                    seat: 0,
                    index: 0,
                    hidden: false,
                },
            ),
        );
        let passed: Verdict = agni_sim::abi::decode(&decide_bytes(&a_move)).unwrap();
        assert!(passed.accept);
        assert!(passed.plugin_state.is_none());
    }

    #[test]
    fn the_sdk_writes_legal_rows_and_arrows_the_hosts_wire_reads_back() {
        use agni_plugin_sdk::view as sdk;
        let view = sdk::PluginView::default()
            .line("turn 3")
            .legal(vec![
                sdk::Legal {
                    card: 70,
                    kinds: vec![
                        sdk::LegalKind::Play { accelerate: true },
                        sdk::LegalKind::React,
                    ],
                    zones: vec![8, 12],
                    hidden: Vec::new(),
                },
                sdk::Legal {
                    card: 75,
                    kinds: vec![
                        sdk::LegalKind::March,
                        sdk::LegalKind::Activate { ability: 2 },
                        sdk::LegalKind::Answer,
                    ],
                    zones: Vec::new(),
                    hidden: Vec::new(),
                },
                sdk::Legal {
                    card: 76,
                    kinds: vec![
                        sdk::LegalKind::Play { accelerate: false },
                        sdk::LegalKind::Hide,
                    ],
                    zones: vec![12],
                    hidden: vec![9, 10],
                },
            ])
            .arrows(vec![
                sdk::Arrow {
                    from: sdk::Origin::Card(71),
                    to: sdk::TargetRef::Card(81),
                    kind: sdk::ArrowKind::Spell,
                },
                sdk::Arrow {
                    from: sdk::Origin::Item(4),
                    to: sdk::TargetRef::Seat(1),
                    kind: sdk::ArrowKind::Ability,
                },
                sdk::Arrow {
                    from: sdk::Origin::Card(50),
                    to: sdk::TargetRef::Zone(9),
                    kind: sdk::ArrowKind::Attack,
                },
                sdk::Arrow {
                    from: sdk::Origin::Card(52),
                    to: sdk::TargetRef::Item(4),
                    kind: sdk::ArrowKind::Counter,
                },
                sdk::Arrow {
                    from: sdk::Origin::Card(50),
                    to: sdk::TargetRef::Card(60),
                    kind: sdk::ArrowKind::Combat,
                },
            ])
            .chain(vec![
                sdk::ChainRow {
                    item: 4,
                    card: Some(71),
                    seat: 0,
                },
                sdk::ChainRow {
                    item: 5,
                    card: None,
                    seat: 1,
                },
            ]);
        let wire = agni_sim::wire::decode_plugin_view(&view.encode())
            .expect("the host reads what the plugin wrote");
        assert_eq!(
            wire.legal,
            vec![
                agni_sim::wire::Legal {
                    card: 70,
                    kinds: vec![
                        agni_sim::wire::LegalKind::Play { accelerate: true },
                        agni_sim::wire::LegalKind::React,
                    ],
                    zones: vec![8, 12],
                    hidden: Vec::new(),
                },
                agni_sim::wire::Legal {
                    card: 75,
                    kinds: vec![
                        agni_sim::wire::LegalKind::March,
                        agni_sim::wire::LegalKind::Activate { ability: 2 },
                        agni_sim::wire::LegalKind::Answer,
                    ],
                    zones: Vec::new(),
                    hidden: Vec::new(),
                },
                agni_sim::wire::Legal {
                    card: 76,
                    kinds: vec![
                        agni_sim::wire::LegalKind::Play { accelerate: false },
                        agni_sim::wire::LegalKind::Hide,
                    ],
                    zones: vec![12],
                    hidden: vec![9, 10],
                },
            ]
        );
        assert_eq!(
            wire.arrows,
            vec![
                agni_sim::wire::Arrow {
                    from: agni_sim::wire::Origin::Card(71),
                    to: agni_sim::wire::TargetRef::Card(81),
                    kind: agni_sim::wire::ArrowKind::Spell,
                },
                agni_sim::wire::Arrow {
                    from: agni_sim::wire::Origin::Item(4),
                    to: agni_sim::wire::TargetRef::Seat(1),
                    kind: agni_sim::wire::ArrowKind::Ability,
                },
                agni_sim::wire::Arrow {
                    from: agni_sim::wire::Origin::Card(50),
                    to: agni_sim::wire::TargetRef::Zone(9),
                    kind: agni_sim::wire::ArrowKind::Attack,
                },
                agni_sim::wire::Arrow {
                    from: agni_sim::wire::Origin::Card(52),
                    to: agni_sim::wire::TargetRef::Item(4),
                    kind: agni_sim::wire::ArrowKind::Counter,
                },
                agni_sim::wire::Arrow {
                    from: agni_sim::wire::Origin::Card(50),
                    to: agni_sim::wire::TargetRef::Card(60),
                    kind: agni_sim::wire::ArrowKind::Combat,
                },
            ]
        );
        assert_eq!(
            wire.chain,
            vec![
                agni_sim::wire::ChainRow {
                    item: 4,
                    card: Some(71),
                    seat: 0,
                },
                agni_sim::wire::ChainRow {
                    item: 5,
                    card: None,
                    seat: 1,
                },
            ]
        );
        let bare = agni_sim::wire::decode_plugin_view(&sdk::PluginView::default().encode())
            .expect("an empty view still decodes");
        assert!(bare.legal.is_empty());
        assert!(bare.arrows.is_empty());
        assert!(bare.chain.is_empty());
    }

    #[test]
    fn the_view_answers_the_engines_real_request_with_affordances() {
        let mut state = seated_state();
        state.plugin_state = ByteBuf::from(GameBlob::start(2, 0, Mode::Enforced).encode());
        let bytes = agni_sim::abi::encode(&agni_sim::abi::PluginViewRequest::of(&state, 0));
        let view = agni_sim::wire::decode_plugin_view(&view_bytes(&bytes)).unwrap();
        assert_eq!(
            view.status[0],
            "turn 1 · {seat 0} · action phase · rules enforced"
        );
        assert_eq!(view.affordances[0].label, "end turn");
        assert_eq!(view.affordances[0].hotkey.as_deref(), Some("space"));
        assert_eq!(
            TurnEvent::decode(&view.affordances[0].data),
            Some(TurnEvent::EndTurn)
        );
        assert_eq!(view.affordances[1].label, "free table");
        assert_eq!(view.affordances[2].label, "concede");
        assert_eq!(view.affordances[3].label, agni_plugin_sdk::manual::DISABLE);
        assert_eq!(
            view.hidden,
            [2, 3],
            "concede and emergency recovery are table-menu verbs"
        );
        assert_eq!(view.prompt, None);
        assert_eq!(view.primary, Some(0));
        let turn = view
            .turn
            .clone()
            .expect("the structured turn crosses the ABI");
        assert_eq!((turn.number, turn.seat), (1, 0));
        assert_eq!(turn.phases.len(), 9);
        assert_eq!(view.seats.len(), 2);
        let mut asked = GameBlob::start(2, 0, Mode::Free);
        asked.prompt = Some(agni_plugin_sdk::prompt::Prompt::new(2, 0, 0, 1));
        state.plugin_state = ByteBuf::from(asked.encode());
        let bytes = agni_sim::abi::encode(&agni_sim::abi::PluginViewRequest::of(&state, 1));
        let view = agni_sim::wire::decode_plugin_view(&view_bytes(&bytes)).unwrap();
        let summary = view
            .prompt
            .clone()
            .expect("the prompt summary crosses the ABI");
        assert_eq!((summary.seat, summary.max, summary.optional), (0, 1, true));
        assert_eq!(view.shown().count(), 0);
        assert_eq!(view.affordances.len(), 2);
        assert_eq!(view.waiting.as_ref().map(|held| held.seat), Some(Some(0)));
        assert_eq!(VIEW, [0xf6]);
        assert_eq!(PLUGIN_ABI_VERSION, agni_plugin_sdk::PLUGIN_ABI_VERSION);
    }
}
