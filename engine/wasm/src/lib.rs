use agni_sim::abi::{
    decode, encode, DecideRequestReply, FoldLogReply, FoldLogRequest, FoldReply, FoldRequest,
    RestoreReply, SnapshotReply, ViewReply, ViewRequest, ENGINE_ABI_VERSION,
};
use agni_sim::engine::{Engine, NativeEngine};
use serde_bytes::ByteBuf;
use std::sync::Mutex;

static ENGINE: Mutex<Option<NativeEngine>> = Mutex::new(None);

#[cfg(target_arch = "wasm32")]
static REPLY: Mutex<Vec<u8>> = Mutex::new(Vec::new());

fn with_engine<T>(work: impl FnOnce(&mut NativeEngine) -> T) -> T {
    let mut guard = ENGINE.lock().unwrap();
    work(guard.get_or_insert_with(NativeEngine::new))
}

pub fn engine_abi_version() -> u32 {
    ENGINE_ABI_VERSION
}

pub fn handle_fold_entry(bytes: &[u8]) -> Vec<u8> {
    let reply: FoldReply = match decode::<FoldRequest>(bytes) {
        None => Err("fold request does not decode".into()),
        Some(request) => with_engine(|engine| {
            engine.fold_entry(
                &request.entry,
                request.verdict,
                request.mode,
                request.viewer,
            )
        })
        .map_err(|fault| fault.0),
    };
    encode(&reply)
}

pub fn handle_fold_log(bytes: &[u8]) -> Vec<u8> {
    let reply: FoldLogReply = match decode::<FoldLogRequest>(bytes) {
        None => Err("fold log request does not decode".into()),
        Some(request) => with_engine(|engine| engine.fold_log(&request.entries, request.viewer))
            .map_err(|fault| fault.0),
    };
    encode(&reply)
}

pub fn handle_decide_request(bytes: &[u8]) -> Vec<u8> {
    let reply: DecideRequestReply = match decode(bytes) {
        None => Err("decide request does not decode".into()),
        Some(entry) => with_engine(|engine| engine.decide_request(&entry))
            .map(|request| request.map(ByteBuf::from))
            .map_err(|fault| fault.0),
    };
    encode(&reply)
}

pub fn handle_snapshot(_bytes: &[u8]) -> Vec<u8> {
    let reply: SnapshotReply = with_engine(|engine| engine.snapshot())
        .map(ByteBuf::from)
        .map_err(|fault| fault.0);
    encode(&reply)
}

pub fn handle_restore(bytes: &[u8]) -> Vec<u8> {
    let reply: RestoreReply = match decode::<ByteBuf>(bytes) {
        None => Err("restore request does not decode".into()),
        Some(snapshot) => with_engine(|engine| engine.restore(&snapshot)).map_err(|fault| fault.0),
    };
    encode(&reply)
}

pub fn handle_view(bytes: &[u8]) -> Vec<u8> {
    let reply: ViewReply = match decode::<ViewRequest>(bytes) {
        None => Err("view request does not decode".into()),
        Some(request) => with_engine(|engine| engine.view(request.viewer)).map_err(|fault| fault.0),
    };
    encode(&reply)
}

#[cfg(target_arch = "wasm32")]
fn stash_reply(reply: Vec<u8>) -> u64 {
    let mut guard = REPLY.lock().unwrap();
    *guard = reply;
    ((guard.as_ptr() as u64) << 32) | guard.len() as u64
}

#[cfg(target_arch = "wasm32")]
mod exports {
    use super::*;
    use std::alloc::Layout;

    fn request(ptr: u32, len: u32) -> Vec<u8> {
        if len == 0 {
            return Vec::new();
        }
        let slice = unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) }.to_vec();
        release(ptr, len);
        slice
    }

    fn release(ptr: u32, len: u32) {
        if len == 0 {
            return;
        }
        let layout = Layout::array::<u8>(len as usize).unwrap();
        unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
    }

    #[no_mangle]
    pub extern "C" fn abi_version() -> u32 {
        engine_abi_version()
    }

    #[no_mangle]
    pub extern "C" fn alloc(len: u32) -> u32 {
        if len == 0 {
            return 0;
        }
        let layout = Layout::array::<u8>(len as usize).unwrap();
        unsafe { std::alloc::alloc(layout) as u32 }
    }

    #[no_mangle]
    pub extern "C" fn dealloc(ptr: u32, len: u32) {
        release(ptr, len);
    }

    #[no_mangle]
    pub extern "C" fn fold_entry(ptr: u32, len: u32) -> u64 {
        stash_reply(handle_fold_entry(&request(ptr, len)))
    }

    #[no_mangle]
    pub extern "C" fn fold_log(ptr: u32, len: u32) -> u64 {
        stash_reply(handle_fold_log(&request(ptr, len)))
    }

    #[no_mangle]
    pub extern "C" fn decide_request(ptr: u32, len: u32) -> u64 {
        stash_reply(handle_decide_request(&request(ptr, len)))
    }

    #[no_mangle]
    pub extern "C" fn snapshot(ptr: u32, len: u32) -> u64 {
        stash_reply(handle_snapshot(&request(ptr, len)))
    }

    #[no_mangle]
    pub extern "C" fn restore(ptr: u32, len: u32) -> u64 {
        stash_reply(handle_restore(&request(ptr, len)))
    }

    #[no_mangle]
    pub extern "C" fn view(ptr: u32, len: u32) -> u64 {
        stash_reply(handle_view(&request(ptr, len)))
    }
}
