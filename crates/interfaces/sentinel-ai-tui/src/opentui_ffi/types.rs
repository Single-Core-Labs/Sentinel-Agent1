pub type NativeHandle = u32;
pub const INVALID_HANDLE: NativeHandle = 0;

pub const fn encode_handle(slot: u16, generation: u16, kind: u8) -> NativeHandle {
    (slot as u32) | ((generation as u32) << 16) | ((kind as u32) << 28)
}

#[repr(C)]
pub struct ExternalBuildOptions {
    pub gpa_safe_stats: bool,
    pub gpa_memory_limit_tracking: bool,
}

#[repr(C)]
pub struct ExternalAllocatorStats {
    pub total_requested_bytes: u64,
    pub active_allocations: u64,
    pub small_allocations: u64,
    pub large_allocations: u64,
    pub requested_bytes_valid: bool,
}

#[repr(C)]
pub struct ExternalRenderStats {
    pub last_frame_time: f64,
    pub average_frame_time: f64,
    pub render_time: f64,
    pub stdout_write_time: f64,
    pub frame_count: u64,
    pub cells_updated: u32,
    pub average_cells_updated: u32,
    pub render_time_valid: bool,
    pub stdout_write_time_valid: bool,
}

#[repr(C)]
pub struct ExternalCapabilities {
    pub kitty_keyboard: bool,
    pub kitty_graphics: bool,
    pub rgb: bool,
    pub ansi256: bool,
    pub unicode: u8,
    pub sgr_pixels: bool,
    pub color_scheme_updates: bool,
    pub explicit_width: bool,
    pub scaled_text: bool,
    pub sixel: bool,
    pub focus_tracking: bool,
    pub sync: bool,
    pub bracketed_paste: bool,
    pub hyperlinks: bool,
    pub osc52: bool,
    pub notifications: bool,
    pub explicit_cursor_positioning: bool,
    pub remote: bool,
    pub multiplexer: u8,
    pub term_name_ptr: *const u8,
    pub term_name_len: usize,
    pub term_version_ptr: *const u8,
    pub term_version_len: usize,
    pub term_from_xtversion: bool,
    pub osc52_support: u8,
}

#[repr(C)]
pub struct CursorStyleOptions {
    pub style: u8,
    pub blinking: u8,
    pub color: *const u16,
    pub cursor: u8,
}

#[repr(C)]
pub struct ExternalCursorState {
    pub x: u32,
    pub y: u32,
    pub visible: bool,
    pub style: u8,
    pub blinking: bool,
    pub r: f32,
    pub g: f32,
    pub b: f32,
    pub a: f32,
}

#[repr(C)]
pub struct ExternalGridDrawOptions {
    pub draw_inner: bool,
    pub draw_outer: bool,
}

#[repr(C)]
pub struct ExternalHighlight {
    pub start: u32,
    pub end: u32,
    pub style_id: u32,
    pub priority: u8,
    pub hl_ref: u16,
}

#[repr(C)]
pub struct ExternalLineInfo {
    pub start_cols_ptr: *const u32,
    pub start_cols_len: u32,
    pub width_cols_ptr: *const u32,
    pub width_cols_len: u32,
    pub sources_ptr: *const u32,
    pub sources_len: u32,
    pub wraps_ptr: *const u32,
    pub wraps_len: u32,
    pub width_cols_max: u32,
}

#[repr(C)]
pub struct ExternalMeasureResult {
    pub line_count: u32,
    pub width_cols_max: u32,
}

#[repr(C)]
pub struct EncodedChar {
    pub width: u8,
    pub char: u32,
}

#[repr(C)]
pub struct ExternalLogicalCursor {
    pub row: u32,
    pub col: u32,
    pub offset: u32,
}

#[repr(C)]
pub struct ExternalVisualCursor {
    pub visual_row: u32,
    pub visual_col: u32,
    pub logical_row: u32,
    pub logical_col: u32,
    pub offset: u32,
}

pub type RgbaColor = [u16; 4];

pub fn pack_rgba(r: u16, g: u16, b: u16, a: u16) -> RgbaColor {
    [r, g, b, a]
}
