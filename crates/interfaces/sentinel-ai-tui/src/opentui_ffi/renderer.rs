use std::marker::PhantomData;

use super::ffi;
use super::types::*;

/// Safe wrapper around an OpenTUI Renderer handle.
///
/// On drop the renderer is destroyed and the terminal is restored.
pub struct Renderer {
    handle: NativeHandle,
    owned: bool,
}

unsafe impl Send for Renderer {}
unsafe impl Sync for Renderer {}

impl Renderer {
    pub unsafe fn from_handle(handle: NativeHandle) -> Self {
        Self {
            handle,
            owned: true,
        }
    }

    pub fn handle(&self) -> NativeHandle {
        self.handle
    }

    pub fn set_background_color(&self, color: RgbaColor) {
        unsafe {
            ffi::setBackgroundColor(self.handle, color.as_ptr());
        }
    }

    pub fn resize(&self, width: u32, height: u32) {
        unsafe {
            ffi::resizeRenderer(self.handle, width, height);
        }
    }

    pub fn get_next_buffer(&self) -> Option<OptimizedBuffer> {
        let h = unsafe { ffi::getNextBuffer(self.handle) };
        if h == INVALID_HANDLE {
            None
        } else {
            Some(unsafe { OptimizedBuffer::from_handle(h) })
        }
    }

    pub fn get_current_buffer(&self) -> Option<OptimizedBuffer> {
        let h = unsafe { ffi::getCurrentBuffer(self.handle) };
        if h == INVALID_HANDLE {
            None
        } else {
            Some(unsafe { OptimizedBuffer::from_handle(h) })
        }
    }

    pub fn render(&self, force: bool) -> u8 {
        unsafe { ffi::render(self.handle, force) }
    }

    pub fn setup_terminal(&self, alt_screen: bool) {
        unsafe { ffi::setupTerminal(self.handle, alt_screen) }
    }

    pub fn restore_terminal(&self) {
        unsafe { ffi::restoreTerminalModes(self.handle) }
    }

    pub fn suspend(&self) {
        unsafe { ffi::suspendRenderer(self.handle) }
    }

    pub fn resume(&self) {
        unsafe { ffi::resumeRenderer(self.handle) }
    }

    pub fn clear(&self) {
        unsafe { ffi::clearTerminal(self.handle) }
    }

    pub fn set_title(&self, title: &str) {
        unsafe {
            ffi::setTerminalTitle(self.handle, title.as_ptr(), title.len() as u32);
        }
    }

    pub fn get_render_stats(&self) -> ExternalRenderStats {
        unsafe {
            let mut stats = std::mem::zeroed();
            ffi::getRenderStats(self.handle, &mut stats);
            stats
        }
    }

    pub fn get_capabilities(&self) -> ExternalCapabilities {
        unsafe {
            let mut caps = std::mem::zeroed();
            ffi::getTerminalCapabilities(self.handle, &mut caps);
            caps
        }
    }

    pub fn set_cursor_position(&self, x: i32, y: i32, visible: bool) {
        unsafe {
            ffi::setCursorPosition(self.handle, x, y, visible);
        }
    }

    pub fn set_cursor_color(&self, color: RgbaColor) {
        unsafe {
            ffi::setCursorColor(self.handle, color.as_ptr());
        }
    }

    pub fn get_cursor_state(&self) -> ExternalCursorState {
        unsafe {
            let mut state = std::mem::zeroed();
            ffi::getCursorState(self.handle, &mut state);
            state
        }
    }

    pub fn add_to_hit_grid(&self, x: i32, y: i32, width: u32, height: u32, id: u32) {
        unsafe {
            ffi::addToHitGrid(self.handle, x, y, width, height, id);
        }
    }

    pub fn clear_hit_grid(&self) {
        unsafe {
            ffi::clearCurrentHitGrid(self.handle);
        }
    }

    pub fn check_hit(&self, x: u32, y: u32) -> u32 {
        unsafe { ffi::checkHit(self.handle, x, y) }
    }

    pub fn enable_mouse(&self, track_movement: bool) {
        unsafe {
            ffi::enableMouse(self.handle, track_movement);
        }
    }

    pub fn disable_mouse(&self) {
        unsafe {
            ffi::disableMouse(self.handle);
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if self.owned && self.handle != INVALID_HANDLE {
            unsafe {
                ffi::restoreTerminalModes(self.handle);
                ffi::destroyRenderer(self.handle);
            }
        }
    }
}

/// Safe wrapper around an OpenTUI OptimizedBuffer handle.
pub struct OptimizedBuffer {
    handle: NativeHandle,
    _not_send: PhantomData<*const ()>,
}

impl OptimizedBuffer {
    pub unsafe fn from_handle(handle: NativeHandle) -> Self {
        Self {
            handle,
            _not_send: PhantomData,
        }
    }

    pub fn handle(&self) -> NativeHandle {
        self.handle
    }

    pub fn width(&self) -> u32 {
        unsafe { ffi::getBufferWidth(self.handle) }
    }

    pub fn height(&self) -> u32 {
        unsafe { ffi::getBufferHeight(self.handle) }
    }

    pub fn clear(&self, bg: RgbaColor) {
        unsafe {
            ffi::bufferClear(self.handle, bg.as_ptr());
        }
    }

    pub fn draw_text(
        &self,
        text: &str,
        x: u32,
        y: u32,
        fg: RgbaColor,
        bg: Option<RgbaColor>,
        attributes: u32,
    ) {
        let bg_ptr = bg.as_ref().map(|c| c.as_ptr()).unwrap_or(std::ptr::null());
        unsafe {
            ffi::bufferDrawText(
                self.handle,
                text.as_ptr(),
                text.len() as u32,
                x,
                y,
                fg.as_ptr(),
                bg_ptr,
                attributes,
            );
        }
    }

    pub fn set_cell(
        &self,
        x: u32,
        y: u32,
        ch: char,
        fg: RgbaColor,
        bg: RgbaColor,
        attributes: u32,
    ) {
        let mut buf = [0u8; 4];
        let _ch_str = ch.encode_utf8(&mut buf);
        unsafe {
            ffi::bufferSetCell(
                self.handle,
                x,
                y,
                ch as u32,
                fg.as_ptr(),
                bg.as_ptr(),
                attributes,
            );
        }
    }

    pub fn fill_rect(&self, x: u32, y: u32, width: u32, height: u32, bg: RgbaColor) {
        unsafe {
            ffi::bufferFillRect(self.handle, x, y, width, height, bg.as_ptr());
        }
    }

    pub fn draw_box(
        &self,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        border_style: u32,
        border_color: RgbaColor,
        bg_color: RgbaColor,
        title: Option<&str>,
    ) {
        let (title_ptr, title_len) = match title {
            Some(t) => (t.as_ptr(), t.len() as u32),
            None => (std::ptr::null(), 0),
        };
        let border_chars: [u32; 8] = [0x250C, 0x2510, 0x2518, 0x2514, 0x2500, 0x2502, 0x2500, 0x2502];
        unsafe {
            ffi::bufferDrawBox(
                self.handle,
                x,
                y,
                width,
                height,
                border_chars.as_ptr(),
                border_style,
                border_color.as_ptr(),
                bg_color.as_ptr(),
                border_color.as_ptr(),
                title_ptr,
                title_len,
                std::ptr::null(),
                0,
            );
        }
    }

    pub fn push_scissor(&self, x: i32, y: i32, width: u32, height: u32) {
        unsafe {
            ffi::bufferPushScissorRect(self.handle, x, y, width, height);
        }
    }

    pub fn pop_scissor(&self) {
        unsafe {
            ffi::bufferPopScissorRect(self.handle);
        }
    }
}

impl Drop for OptimizedBuffer {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE {
            unsafe {
                ffi::destroyOptimizedBuffer(self.handle);
            }
        }
    }
}

/// Safe wrapper around an OpenTUI TextBuffer handle.
pub struct TextBuffer {
    handle: NativeHandle,
}

impl TextBuffer {
    pub unsafe fn from_handle(handle: NativeHandle) -> Self {
        Self { handle }
    }

    pub fn handle(&self) -> NativeHandle {
        self.handle
    }

    pub fn append(&self, text: &str) {
        unsafe {
            ffi::textBufferAppend(self.handle, text.as_ptr(), text.len() as u32);
        }
    }

    pub fn clear(&self) {
        unsafe {
            ffi::textBufferClear(self.handle);
        }
    }

    pub fn line_count(&self) -> u32 {
        unsafe { ffi::textBufferGetLineCount(self.handle) }
    }

    pub fn get_plain_text(&self) -> String {
        let len = unsafe { ffi::textBufferGetLength(self.handle) } as usize;
        let mut buf = vec![0u8; len + 1];
        let written = unsafe { ffi::textBufferGetPlainText(self.handle, buf.as_mut_ptr(), len as u32) };
        buf.truncate(written as usize);
        String::from_utf8_lossy(&buf).to_string()
    }

    pub fn create_view(&self) -> TextBufferView {
        let h = unsafe { ffi::createTextBufferView(self.handle) };
        unsafe { TextBufferView::from_handle(h) }
    }
}

impl Drop for TextBuffer {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE {
            unsafe {
                ffi::destroyTextBuffer(self.handle);
            }
        }
    }
}

/// Safe wrapper around an OpenTUI TextBufferView handle.
pub struct TextBufferView {
    handle: NativeHandle,
}

impl TextBufferView {
    pub unsafe fn from_handle(handle: NativeHandle) -> Self {
        Self { handle }
    }

    pub fn set_viewport(&self, x: u32, y: u32, width: u32, height: u32) {
        unsafe {
            ffi::textBufferViewSetViewport(self.handle, x, y, width, height);
        }
    }

    pub fn set_wrap_width(&self, width: u32) {
        unsafe {
            ffi::textBufferViewSetWrapWidth(self.handle, width);
        }
    }

    pub fn virtual_line_count(&self) -> u32 {
        unsafe { ffi::textBufferViewGetVirtualLineCount(self.handle) }
    }

    pub fn measure(&self, width: u32, height: u32) -> Option<ExternalMeasureResult> {
        let mut result = unsafe { std::mem::zeroed() };
        let ok = unsafe {
            ffi::textBufferViewMeasureForDimensions(self.handle, width, height, &mut result)
        };
        if ok { Some(result) } else { None }
    }

    pub fn draw(&self, buffer: &OptimizedBuffer, x: i32, y: i32) {
        unsafe {
            ffi::bufferDrawTextBufferView(buffer.handle(), self.handle, x, y);
        }
    }
}

impl Drop for TextBufferView {
    fn drop(&mut self) {
        if self.handle != INVALID_HANDLE {
            unsafe {
                ffi::destroyTextBufferView(self.handle);
            }
        }
    }
}
