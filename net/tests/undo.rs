use agni_core::{CardFace, CardId, Zone};
use agni_net::session::{ClientSession, HostSession, WireIntent};

fn table() -> (HostSession, ClientSession) {
    let mut host = HostSession::new("host");
    host.join("guest").unwrap();
    host.deal(0, vec![CardFace::named("private host card")])
        .unwrap();
    host.deal(1, vec![CardFace::named("private guest card")])
        .unwrap();
    let mut client = ClientSession::from_welcome(1, host.roster(), host.log().to_vec());
    client.add_faces(host.faces_owed_to(1));
    (host, client)
}

fn play(host: &mut HostSession, client: &mut ClientSession, card: u32) {
    for entry in host
        .intent(
            card as u8,
            WireIntent::Move {
                card,
                to: Zone::Board,
                seat: card as u8,
                index: 0,
            },
        )
        .unwrap()
    {
        assert!(client.apply(entry).unwrap());
    }
}

#[test]
fn agreement_rolls_back_whole_actions_and_restores_private_faces() {
    let (mut host, mut client) = table();
    let before = host.state().clone();
    play(&mut host, &mut client, 0);
    play(&mut host, &mut client, 1);
    assert_eq!(host.undo_status().available, 2);
    assert!(client
        .table()
        .get(CardId(0))
        .unwrap()
        .face
        .name
        .contains("host"));
    let revision = host.undo_status().revision;
    assert!(!host.request_undo(1, 2, revision).unwrap());
    let proposal = host.undo_status().proposal.unwrap();
    assert_eq!(proposal.waiting, vec![0]);
    assert_ne!(host.state(), &before);
    assert!(host.vote_undo(1, proposal.id, true).is_err());
    assert!(host.vote_undo(0, proposal.id, true).unwrap());
    client
        .rollback(host.state().next_seq, host.faces_owed_to(1))
        .unwrap();
    assert_eq!(host.state(), &before);
    assert_eq!(host.state(), client.state());
    assert!(client.table().get(CardId(0)).unwrap().face.is_hidden());
    assert_eq!(
        client.table().get(CardId(1)).unwrap().face.name,
        "private guest card"
    );
    assert_eq!(host.undo_status().available, 0);
    assert!(host.vote_undo(0, proposal.id, true).is_err());
    play(&mut host, &mut client, 0);
    assert_eq!(host.state(), client.state());
}

#[test]
fn refusal_invalid_counts_and_stale_requests_do_not_change_the_table() {
    let (mut host, mut client) = table();
    play(&mut host, &mut client, 0);
    let before = host.state().clone();
    let revision = host.undo_status().revision;
    assert!(host.request_undo(0, 0, revision).is_err());
    assert!(host.request_undo(0, 2, revision).is_err());
    assert!(host.request_undo(9, 1, revision).is_err());
    assert!(host.request_undo(0, 1, revision - 1).is_err());
    host.request_undo(0, 1, revision).unwrap();
    assert!(host.request_undo(1, 1, revision).is_err());
    let id = host.undo_status().proposal.unwrap().id;
    assert!(!host.vote_undo(1, id, false).unwrap());
    assert_eq!(host.state(), &before);
    assert!(host.undo_status().proposal.is_none());
}

#[test]
fn advancing_or_disconnecting_cancels_the_proposal() {
    let (mut host, mut client) = table();
    play(&mut host, &mut client, 0);
    host.request_undo(0, 1, host.undo_status().revision)
        .unwrap();
    let id = host.undo_status().proposal.unwrap().id;
    play(&mut host, &mut client, 1);
    assert!(host.undo_status().proposal.is_none());
    assert!(host.vote_undo(1, id, true).is_err());
    host.request_undo(0, 1, host.undo_status().revision)
        .unwrap();
    host.disconnect(1);
    assert!(host.undo_status().proposal.is_none());
    assert!(host
        .request_undo(0, 1, host.undo_status().revision)
        .is_err());
}

#[test]
fn every_other_player_must_agree_and_setup_is_a_history_boundary() {
    let (mut host, _) = table();
    host.join("third").unwrap();
    let mut client = ClientSession::from_welcome(1, host.roster(), host.log().to_vec());
    play(&mut host, &mut client, 0);
    host.request_undo(0, 1, host.undo_status().revision)
        .unwrap();
    let id = host.undo_status().proposal.unwrap().id;
    assert!(!host.vote_undo(1, id, true).unwrap());
    assert!(host.vote_undo(1, id, true).is_err());
    assert!(host.vote_undo(2, id, true).unwrap());
    client
        .rollback(host.state().next_seq, host.faces_owed_to(1))
        .unwrap();
    play(&mut host, &mut client, 0);
    host.deal(2, vec![CardFace::named("new card")]).unwrap();
    assert_eq!(host.undo_status().available, 0);
}

#[test]
fn a_host_alone_undoes_without_a_vote_and_bad_replica_boundaries_are_atomic() {
    let mut host = HostSession::new("host");
    host.deal(0, vec![CardFace::named("card")]).unwrap();
    let before = host.state().clone();
    host.intent(
        0,
        WireIntent::Move {
            card: 0,
            to: Zone::Board,
            seat: 0,
            index: 0,
        },
    )
    .unwrap();
    let mut client = ClientSession::from_welcome(0, host.roster(), host.log().to_vec());
    let replica_before = client.state().clone();
    assert!(client.rollback(0, vec![]).is_err());
    assert!(client.rollback(u64::MAX, vec![]).is_err());
    assert_eq!(client.state(), &replica_before);
    assert!(host
        .request_undo(0, 1, host.undo_status().revision)
        .unwrap());
    assert_eq!(host.state(), &before);
}

#[test]
fn undo_history_is_bounded_and_refused_intents_do_not_count() {
    let (mut host, mut client) = table();
    play(&mut host, &mut client, 0);
    for n in 0..70 {
        host.intent(
            0,
            WireIntent::Annotate {
                card: 0,
                key: "mark".into(),
                value: Some(vec![n].into()),
            },
        )
        .unwrap();
    }
    let revision = host.undo_status().revision;
    assert_eq!(host.undo_status().available, 64);
    assert!(host
        .intent(
            0,
            WireIntent::Move {
                card: u32::MAX,
                to: Zone::Board,
                seat: 0,
                index: 0
            }
        )
        .is_err());
    assert_eq!(host.undo_status().revision, revision);
    assert_eq!(host.undo_status().available, 64);
    assert!(host.request_undo(0, 65, revision).is_err());
    host.request_undo(0, 64, revision).unwrap();
    let id = host.undo_status().proposal.unwrap().id;
    assert!(host.vote_undo(1, id, true).unwrap());
    assert!(host.undo_status().revision > revision);
}
