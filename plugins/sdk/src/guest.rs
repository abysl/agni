#[macro_export]
macro_rules! export_plugin {
    (manifest: $manifest:expr, decide: $decide:expr, view: $view:expr $(,)?) => {
        #[cfg(target_arch = "wasm32")]
        mod agni_plugin_exports {
            use std::alloc::Layout;
            use std::sync::Mutex;

            static REPLY: Mutex<Vec<u8>> = Mutex::new(Vec::new());

            fn release(ptr: u32, len: u32) {
                if len == 0 {
                    return;
                }
                let layout = Layout::array::<u8>(len as usize).unwrap();
                unsafe { std::alloc::dealloc(ptr as *mut u8, layout) };
            }

            fn take(ptr: u32, len: u32) -> Vec<u8> {
                if len == 0 {
                    return Vec::new();
                }
                let bytes =
                    unsafe { std::slice::from_raw_parts(ptr as *const u8, len as usize) }.to_vec();
                release(ptr, len);
                bytes
            }

            fn reply(bytes: Vec<u8>) -> u64 {
                let mut guard = REPLY.lock().unwrap();
                *guard = bytes;
                ((guard.as_ptr() as u64) << 32) | guard.len() as u64
            }

            fn packed(bytes: &'static [u8]) -> u64 {
                ((bytes.as_ptr() as u64) << 32) | bytes.len() as u64
            }

            #[no_mangle]
            pub extern "C" fn abi_version() -> u32 {
                $crate::PLUGIN_ABI_VERSION
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
            pub extern "C" fn manifest(ptr: u32, len: u32) -> u64 {
                release(ptr, len);
                packed($manifest)
            }

            #[no_mangle]
            pub extern "C" fn decide(ptr: u32, len: u32) -> u64 {
                let request = take(ptr, len);
                reply(($decide)(&request))
            }

            #[no_mangle]
            pub extern "C" fn view(ptr: u32, len: u32) -> u64 {
                let request = take(ptr, len);
                reply(($view)(&request))
            }
        }
    };
}
