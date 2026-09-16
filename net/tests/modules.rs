use agni_net::session::{
    decode_client, decode_host, encode_client, encode_host, engine_blob_ref, genesis_engine_pin,
    genesis_plugin_pin, module_hash, ClientMsg, HostMsg, HostSession, ModuleInbox,
    ModuleTransferError, MAX_OPEN_ASSEMBLIES, MODULE_CHUNK_BYTES,
};
use agni_sim::engine::{
    Engine, EngineFault, FoldLogOutcome, FoldMode, FoldOutcome, NativeEngine, PluginModule,
};
use agni_sim::log::{LogEntry, TableConfig, Verdict};
use agni_sim::view::TableView;

struct Pinned {
    inner: NativeEngine,
    hash: [u8; 32],
}

impl Engine for Pinned {
    fn fold_entry(
        &mut self,
        entry: &LogEntry,
        verdict: Option<Verdict>,
        mode: FoldMode,
        viewer: u8,
    ) -> Result<FoldOutcome, EngineFault> {
        self.inner.fold_entry(entry, verdict, mode, viewer)
    }

    fn fold_log(
        &mut self,
        entries: &[LogEntry],
        viewer: u8,
    ) -> Result<FoldLogOutcome, EngineFault> {
        self.inner.fold_log(entries, viewer)
    }

    fn decide_request(&mut self, entry: &LogEntry) -> Result<Option<Vec<u8>>, EngineFault> {
        self.inner.decide_request(entry)
    }

    fn snapshot(&mut self) -> Result<Vec<u8>, EngineFault> {
        self.inner.snapshot()
    }

    fn restore(&mut self, bytes: &[u8]) -> Result<(), EngineFault> {
        self.inner.restore(bytes)
    }

    fn view(&mut self, viewer: u8) -> Result<TableView, EngineFault> {
        self.inner.view(viewer)
    }

    fn engine_hash(&self) -> Option<[u8; 32]> {
        Some(self.hash)
    }
}

struct Accepting {
    hash: [u8; 32],
}

impl PluginModule for Accepting {
    fn decide(&mut self, _request: &[u8]) -> Result<Verdict, EngineFault> {
        Ok(Verdict::accept())
    }

    fn module_hash(&self) -> Option<[u8; 32]> {
        Some(self.hash)
    }
}

fn engine_bytes() -> Vec<u8> {
    (0..(MODULE_CHUNK_BYTES * 2 + 17))
        .map(|i| (i % 251) as u8)
        .collect()
}

fn plugin_bytes() -> Vec<u8> {
    b"a small plugin".to_vec()
}

fn host_serving() -> (HostSession, [u8; 32], [u8; 32]) {
    let engine = engine_bytes();
    let plugin = plugin_bytes();
    let engine_hash = module_hash(&engine);
    let plugin_hash = module_hash(&plugin);
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig::default(),
        Box::new(Pinned {
            inner: NativeEngine::new(),
            hash: engine_hash,
        }),
        Some(Box::new(Accepting { hash: plugin_hash })),
    )
    .unwrap();
    assert_eq!(host.serve_module(engine), Some(engine_hash));
    assert_eq!(host.serve_module(plugin), Some(plugin_hash));
    (host, engine_hash, plugin_hash)
}

fn ask(host: &HostSession, hash: [u8; 32]) -> Vec<HostMsg> {
    let framed = encode_client(&ClientMsg::NeedModule { hash });
    let ClientMsg::NeedModule { hash } = decode_client(&framed).unwrap() else {
        panic!("a need-module frame");
    };
    host.module_frames(hash)
        .into_iter()
        .map(|msg| decode_host(&encode_host(&msg)).unwrap())
        .collect()
}

#[test]
fn the_host_serves_both_pinned_modules_in_chunks_and_refuses_anything_else() {
    let (host, engine_hash, plugin_hash) = host_serving();
    assert_eq!(
        genesis_engine_pin(host.log()),
        Some(engine_blob_ref(engine_hash))
    );
    assert_eq!(
        genesis_plugin_pin(host.log()),
        Some(engine_blob_ref(plugin_hash))
    );
    let mut served = host.served_modules();
    served.sort();
    let mut expected = vec![engine_hash, plugin_hash];
    expected.sort();
    assert_eq!(served, expected);

    let frames = ask(&host, engine_hash);
    assert_eq!(frames.len(), 3);
    let mut inbox = ModuleInbox::new();
    for frame in &frames[..2] {
        assert_eq!(inbox.receive(frame).unwrap(), None);
    }
    assert_eq!(
        inbox.progress(engine_hash),
        Some((MODULE_CHUNK_BYTES * 2, engine_bytes().len()))
    );
    assert_eq!(inbox.receive(&frames[2]).unwrap(), Some(engine_hash));
    assert_eq!(inbox.progress(engine_hash), None);
    assert_eq!(inbox.bytes(engine_hash), Some(engine_bytes().as_slice()));

    let frames = ask(&host, plugin_hash);
    assert_eq!(frames.len(), 1);
    assert_eq!(inbox.receive(&frames[0]).unwrap(), Some(plugin_hash));
    assert_eq!(inbox.take(plugin_hash), Some(plugin_bytes()));
    assert_eq!(inbox.take(plugin_hash), None);

    let stranger = [0xaa; 32];
    let refusal = ask(&host, stranger);
    let [HostMsg::NoModule { hash, reason }] = refusal.as_slice() else {
        panic!("one refusal frame, got {refusal:?}");
    };
    assert_eq!(*hash, stranger);
    assert!(reason.contains("pinned no module"), "{reason}");
    assert!(reason.contains(&agni_net::session::hash_hex(&stranger)));
}

#[test]
fn a_pinned_module_the_host_never_loaded_bytes_for_is_refused_by_name() {
    let plugin = plugin_bytes();
    let plugin_hash = module_hash(&plugin);
    let engine_hash = module_hash(&engine_bytes());
    let mut host = HostSession::with_engine(
        "rae",
        TableConfig::default(),
        Box::new(Pinned {
            inner: NativeEngine::new(),
            hash: engine_hash,
        }),
        Some(Box::new(Accepting { hash: plugin_hash })),
    )
    .unwrap();
    assert_eq!(host.serve_module(b"not what genesis pinned".to_vec()), None);
    assert!(host.served_modules().is_empty());
    let refusal = host.module_frames(engine_hash);
    let [HostMsg::NoModule { hash, reason }] = refusal.as_slice() else {
        panic!("a refusal");
    };
    assert_eq!(*hash, engine_hash);
    assert!(reason.contains("holds no bytes"), "{reason}");
    host.serve_module(plugin);
    assert!(matches!(
        host.module_frames(plugin_hash).as_slice(),
        [HostMsg::Module { .. }]
    ));
}

#[test]
fn the_inbox_rejects_a_hash_mismatch_and_out_of_order_chunks() {
    let bytes = plugin_bytes();
    let wrong = [0x11; 32];
    let mut inbox = ModuleInbox::new();
    let error = inbox
        .chunk(wrong, bytes.len() as u32, 0, &bytes)
        .unwrap_err();
    assert_eq!(
        error,
        ModuleTransferError::Mismatch {
            hash: wrong,
            got: module_hash(&bytes)
        }
    );
    assert!(error.to_string().contains("refusing"));
    assert_eq!(inbox.bytes(wrong), None);

    let big = engine_bytes();
    let hash = module_hash(&big);
    let total = big.len() as u32;
    assert_eq!(
        inbox
            .chunk(hash, total, 0, &big[..MODULE_CHUNK_BYTES])
            .unwrap(),
        None
    );
    let error = inbox
        .chunk(hash, total, 5, &big[5..MODULE_CHUNK_BYTES])
        .unwrap_err();
    assert!(matches!(
        error,
        ModuleTransferError::Chunk { offset: 5, .. }
    ));
    assert_eq!(
        inbox.progress(hash),
        None,
        "a broken transfer is dropped whole"
    );

    let oversized = inbox
        .chunk(hash, u32::MAX, 0, &big[..MODULE_CHUNK_BYTES])
        .unwrap_err();
    assert!(matches!(oversized, ModuleTransferError::Oversized { .. }));
    let beyond = inbox
        .chunk(hash, total, 0, &big[..MODULE_CHUNK_BYTES + 1])
        .unwrap_err();
    assert!(matches!(beyond, ModuleTransferError::Chunk { .. }));

    for frame in agni_net::session::module_chunks(hash, &big) {
        inbox.receive(&frame).unwrap();
    }
    assert_eq!(inbox.take(hash), Some(big));
}

#[test]
fn the_inbox_grows_as_chunks_land_and_refuses_a_third_open_transfer() {
    let mut inbox = ModuleInbox::new();
    let first = [0x01; 32];
    let second = [0x02; 32];
    let third = [0x03; 32];
    let total = (MODULE_CHUNK_BYTES * 4) as u32;
    let chunk = vec![0u8; MODULE_CHUNK_BYTES];
    assert_eq!(inbox.chunk(first, total, 0, &chunk).unwrap(), None);
    assert_eq!(inbox.chunk(second, total, 0, &chunk).unwrap(), None);
    assert_eq!(
        inbox.chunk(third, total, 0, &chunk).unwrap_err(),
        ModuleTransferError::TooManyOpen { hash: third }
    );
    assert_eq!(inbox.progress(third), None);
    assert_eq!(
        inbox
            .chunk(first, total, MODULE_CHUNK_BYTES as u32, &chunk)
            .unwrap(),
        None
    );
    assert_eq!(
        inbox.progress(first),
        Some((MODULE_CHUNK_BYTES * 2, total as usize))
    );
    inbox.forget(first);
    assert_eq!(inbox.chunk(third, total, 0, &chunk).unwrap(), None);
    assert_eq!(
        inbox.progress(third),
        Some((MODULE_CHUNK_BYTES, total as usize))
    );
    assert_eq!(MAX_OPEN_ASSEMBLIES, 2);
}
