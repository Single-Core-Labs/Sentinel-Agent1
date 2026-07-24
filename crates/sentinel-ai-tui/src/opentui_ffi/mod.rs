pub mod types;
pub mod ffi;
pub mod renderer;

pub use types::*;
pub use renderer::{Renderer, OptimizedBuffer, TextBuffer, TextBufferView};

pub fn set_log_callback(callback: ffi::LogCallback) {
    unsafe { ffi::setLogCallback(Some(callback)) }
}

pub fn clear_log_callback() {
    unsafe { ffi::setLogCallback(None) }
}

pub fn get_arena_allocated_bytes() -> u64 {
    unsafe { ffi::getArenaAllocatedBytes() }
}

pub fn create_event_sink(callback: ffi::EventCallback) -> NativeHandle {
    unsafe { ffi::createEventSink(Some(callback)) }
}

pub fn destroy_event_sink(handle: NativeHandle) {
    unsafe { ffi::destroyEventSink(handle) }
}

pub fn link_alloc(url: &str) -> u32 {
    unsafe { ffi::linkAlloc(url.as_ptr(), url.len() as u32) }
}

pub fn link_get_url(id: u32) -> Option<String> {
    let mut buf = [0u8; 4096];
    let len = unsafe { ffi::linkGetUrl(id, buf.as_mut_ptr(), buf.len() as u32) };
    if len == 0 {
        None
    } else {
        Some(String::from_utf8_lossy(&buf[..len as usize]).to_string())
    }
}
