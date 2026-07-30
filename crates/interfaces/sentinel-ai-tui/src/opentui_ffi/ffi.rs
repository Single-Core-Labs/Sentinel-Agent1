use super::types::*;
use std::ffi::c_void;

pub type LogCallback = unsafe extern "C" fn(level: u8, msg_ptr: *const u8, msg_len: u32);
pub type EventCallback =
    unsafe extern "C" fn(name_ptr: *const u8, name_len: u32, data_ptr: *const u8, data_len: u32);

extern "C" {
    // ── Logging ──
    pub fn setLogCallback(callback: Option<LogCallback>);

    // ── Renderer ──
    pub fn createRenderer(
        width: u32,
        height: u32,
        buffered_destination_kind: u8,
        remote_mode_value: u8,
        feed_ptr: *mut c_void,
    ) -> NativeHandle;
    pub fn destroyRenderer(renderer_handle: NativeHandle);
    pub fn render(renderer_handle: NativeHandle, force: bool) -> u8;
    pub fn resizeRenderer(renderer_handle: NativeHandle, width: u32, height: u32);
    pub fn setupTerminal(renderer_handle: NativeHandle, use_alternate_screen: bool);
    pub fn restoreTerminalModes(renderer_handle: NativeHandle);
    pub fn getNextBuffer(renderer_handle: NativeHandle) -> NativeHandle;
    pub fn getCurrentBuffer(renderer_handle: NativeHandle) -> NativeHandle;
    pub fn setBackgroundColor(renderer_handle: NativeHandle, color: *const u16);
    pub fn setUseThread(renderer_handle: NativeHandle, use_thread: bool);
    pub fn setClearOnShutdown(renderer_handle: NativeHandle, clear: bool);
    pub fn getRenderStats(
        renderer_handle: NativeHandle,
        out_ptr: *mut ExternalRenderStats,
    );
    pub fn getTerminalCapabilities(
        renderer_handle: NativeHandle,
        caps_ptr: *mut ExternalCapabilities,
    );
    pub fn suspendRenderer(renderer_handle: NativeHandle);
    pub fn resumeRenderer(renderer_handle: NativeHandle);
    pub fn clearTerminal(renderer_handle: NativeHandle);
    pub fn setTerminalTitle(
        renderer_handle: NativeHandle,
        title_ptr: *const u8,
        title_len: u32,
    );
    pub fn updateStats(
        renderer_handle: NativeHandle,
        time: f64,
        fps: u32,
        frame_callback_time: f64,
    );

    // ── Optimized Buffer ──
    pub fn createOptimizedBuffer(
        width: u32,
        height: u32,
        respect_alpha: bool,
        width_method: u8,
        id_ptr: *const u8,
        id_len: u32,
    ) -> NativeHandle;
    pub fn destroyOptimizedBuffer(buffer_handle: NativeHandle);
    pub fn getBufferWidth(buffer_handle: NativeHandle) -> u32;
    pub fn getBufferHeight(buffer_handle: NativeHandle) -> u32;
    pub fn bufferClear(buffer_handle: NativeHandle, bg: *const u16);
    pub fn bufferDrawText(
        buffer_handle: NativeHandle,
        text: *const u8,
        text_len: u32,
        x: u32,
        y: u32,
        fg: *const u16,
        bg: *const u16,
        attributes: u32,
    );
    pub fn bufferSetCell(
        buffer_handle: NativeHandle,
        x: u32,
        y: u32,
        char: u32,
        fg: *const u16,
        bg: *const u16,
        attributes: u32,
    );
    pub fn bufferFillRect(
        buffer_handle: NativeHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        bg: *const u16,
    );
    pub fn bufferDrawBox(
        buffer_handle: NativeHandle,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        border_chars: *const u32,
        packed_options: u32,
        border_color: *const u16,
        background_color: *const u16,
        title_color: *const u16,
        title: *const u8,
        title_len: u32,
        bottom_title: *const u8,
        bottom_title_len: u32,
    );
    pub fn bufferResize(buffer_handle: NativeHandle, width: u32, height: u32);
    pub fn bufferGetCharPtr(buffer_handle: NativeHandle) -> *mut u32;
    pub fn bufferGetFgPtr(buffer_handle: NativeHandle) -> *mut u16;
    pub fn bufferGetBgPtr(buffer_handle: NativeHandle) -> *mut u16;
    pub fn bufferGetAttributesPtr(buffer_handle: NativeHandle) -> *mut u32;
    pub fn bufferPushScissorRect(
        buffer_handle: NativeHandle,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
    );
    pub fn bufferPopScissorRect(buffer_handle: NativeHandle);
    pub fn bufferClearScissorRects(buffer_handle: NativeHandle);

    // ── Text Buffer ──
    pub fn createTextBuffer(width_method: u8) -> NativeHandle;
    pub fn destroyTextBuffer(tb_handle: NativeHandle);
    pub fn textBufferGetLength(tb_handle: NativeHandle) -> u32;
    pub fn textBufferAppend(tb_handle: NativeHandle, data_ptr: *const u8, data_len: u32);
    pub fn textBufferGetLineCount(tb_handle: NativeHandle) -> u32;
    pub fn textBufferGetPlainText(
        tb_handle: NativeHandle,
        out_ptr: *mut u8,
        max_len: u32,
    ) -> u32;
    pub fn textBufferClear(tb_handle: NativeHandle);
    pub fn textBufferReset(tb_handle: NativeHandle);
    pub fn textBufferSetDefaultFg(tb_handle: NativeHandle, fg: *const u16);
    pub fn textBufferSetDefaultBg(tb_handle: NativeHandle, bg: *const u16);
    pub fn textBufferSetDefaultAttributes(tb_handle: NativeHandle, attr: *const u32);

    // ── Text Buffer View ──
    pub fn createTextBufferView(tb_handle: NativeHandle) -> NativeHandle;
    pub fn destroyTextBufferView(view_handle: NativeHandle);
    pub fn textBufferViewSetViewport(
        view_handle: NativeHandle,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
    );
    pub fn textBufferViewSetWrapWidth(view_handle: NativeHandle, width: u32);
    pub fn textBufferViewGetVirtualLineCount(view_handle: NativeHandle) -> u32;
    pub fn textBufferViewMeasureForDimensions(
        view_handle: NativeHandle,
        width: u32,
        height: u32,
        out_ptr: *mut ExternalMeasureResult,
    ) -> bool;
    pub fn bufferDrawTextBufferView(
        buffer_handle: NativeHandle,
        view_handle: NativeHandle,
        x: i32,
        y: i32,
    );

    // ── Cursor ──
    pub fn setCursorPosition(
        renderer_handle: NativeHandle,
        x: i32,
        y: i32,
        visible: bool,
    );
    pub fn setCursorColor(renderer_handle: NativeHandle, color: *const u16);
    pub fn setCursorStyleOptions(
        renderer_handle: NativeHandle,
        options: *const CursorStyleOptions,
    );
    pub fn getCursorState(
        renderer_handle: NativeHandle,
        out_ptr: *mut ExternalCursorState,
    );

    // ── Hit Grid ──
    pub fn addToHitGrid(
        renderer_handle: NativeHandle,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        id: u32,
    );
    pub fn clearCurrentHitGrid(renderer_handle: NativeHandle);
    pub fn checkHit(renderer_handle: NativeHandle, x: u32, y: u32) -> u32;

    // ── Link API ──
    pub fn linkAlloc(url_ptr: *const u8, url_len: u32) -> u32;
    pub fn linkGetUrl(id: u32, out_ptr: *mut u8, max_len: u32) -> u32;
    pub fn attributesWithLink(base_attributes: u32, link_id: u32) -> u32;
    pub fn attributesGetLinkId(attributes: u32) -> u32;

    // ── Allocator ──
    pub fn getArenaAllocatedBytes() -> u64;
    pub fn getBuildOptions(out_ptr: *mut ExternalBuildOptions);
    pub fn getAllocatorStats(out_ptr: *mut ExternalAllocatorStats);

    // ── Terminal Control ──
    pub fn enableMouse(renderer_handle: NativeHandle, enable_movement: bool);
    pub fn disableMouse(renderer_handle: NativeHandle);
    pub fn setDebugOverlay(
        renderer_handle: NativeHandle,
        enabled: bool,
        corner: u8,
    );
    pub fn enableKittyKeyboard(renderer_handle: NativeHandle, flags: u8);
    pub fn disableKittyKeyboard(renderer_handle: NativeHandle);
    pub fn triggerNotification(
        renderer_handle: NativeHandle,
        message_ptr: *const u8,
        message_len: u32,
        title_ptr: *const u8,
        title_len: u32,
    ) -> bool;
    pub fn copyToClipboardOSC52(
        renderer_handle: NativeHandle,
        target: u8,
        text_ptr: *const u8,
        text_len: u32,
    ) -> bool;

    // ── Event Sink ──
    pub fn createEventSink(callback: Option<EventCallback>) -> NativeHandle;
    pub fn destroyEventSink(sink_handle: NativeHandle);

    // ── Syntax Style ──
    pub fn createSyntaxStyle() -> NativeHandle;
    pub fn destroySyntaxStyle(style_handle: NativeHandle);
    pub fn syntaxStyleRegister(
        style_handle: NativeHandle,
        name_ptr: *const u8,
        name_len: u32,
        fg: *const u16,
        bg: *const u16,
        attributes: u32,
    ) -> u32;

    // ── Unicode ──
    pub fn encodeUnicode(
        text_ptr: *const u8,
        text_len: u32,
        out_ptr: *mut *mut EncodedChar,
        out_len_ptr: *mut usize,
        width_method: u8,
    ) -> bool;
    pub fn freeUnicode(chars_ptr: *const EncodedChar, chars_len: u32);
}
