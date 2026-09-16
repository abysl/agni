#![no_std]

use core::sync::atomic::{AtomicUsize, Ordering};

#[panic_handler]
fn panic(_: &core::panic::PanicInfo) -> ! {
    loop {}
}

static mut ARENA: [u8; 65536] = [0; 65536];
static NEXT: AtomicUsize = AtomicUsize::new(0);

static MANIFEST: [u8; 20] = *b"\xa1\x64name\x6dhello-decider";

#[unsafe(no_mangle)]
pub extern "C" fn abi_version() -> u32 {
    0
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(len: u32) -> u32 {
    let at = NEXT.fetch_add(len as usize, Ordering::Relaxed);
    unsafe { (&raw mut ARENA as *mut u8).add(at) as u32 }
}

#[unsafe(no_mangle)]
pub extern "C" fn dealloc(_ptr: u32, _len: u32) {}

#[unsafe(no_mangle)]
pub extern "C" fn manifest() -> u64 {
    let ptr = &raw const MANIFEST as *const u8 as u64;
    (ptr << 32) | MANIFEST.len() as u64
}

fn fold(ptr: u32, len: u32) -> u64 {
    let input = unsafe { core::slice::from_raw_parts(ptr as *const u8, len as usize) };
    let mut acc: u64 = 0xcbf29ce484222325;
    for byte in input {
        acc ^= *byte as u64;
        acc = acc.wrapping_mul(0x100000001b3);
    }
    acc
}

#[unsafe(no_mangle)]
pub extern "C" fn decide(ptr: u32, len: u32) -> u64 {
    let acc = fold(ptr, len);
    let verdict = if acc & 1 == 0 { 1u8 } else { 0u8 };
    let out = alloc(9);
    unsafe {
        *(out as *mut u8) = verdict;
        core::ptr::copy_nonoverlapping(acc.to_le_bytes().as_ptr(), (out + 1) as *mut u8, 8);
    }
    ((out as u64) << 32) | 9
}

#[unsafe(no_mangle)]
pub extern "C" fn view(ptr: u32, len: u32) -> u64 {
    let acc = fold(ptr, len);
    let out = alloc(8);
    unsafe {
        core::ptr::copy_nonoverlapping(acc.to_le_bytes().as_ptr(), out as *mut u8, 8);
    }
    ((out as u64) << 32) | 8
}

#[unsafe(no_mangle)]
pub extern "C" fn spin() -> u32 {
    let mut n: u32 = 1;
    loop {
        n = n.wrapping_mul(31).wrapping_add(7);
        if n == 0 {
            return n;
        }
    }
}
