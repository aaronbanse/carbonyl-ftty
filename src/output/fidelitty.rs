use libc::c_void;

#[repr(C)]
#[derive(Copy, Clone)]
pub struct FttyUnicodePixel {
    pub br: u8,
    pub bg: u8,
    pub bb: u8,
    pub fr: u8,
    pub fg: u8,
    pub fb: u8,
    pub _pad: u16,
    pub codepoint: u32,
}

pub type FttyContext = *mut c_void;
pub type FttyPipeline = *mut c_void;

extern "C" {
    pub fn ftty_context_create(max_pipelines: u8) -> FttyContext;
    pub fn ftty_context_destroy(ctx: FttyContext);

    pub fn ftty_context_create_render_pipeline(
        ctx: FttyContext,
        w: u16,
        h: u16,
    ) -> FttyPipeline;
    pub fn ftty_context_destroy_render_pipeline(ctx: FttyContext, handle: FttyPipeline);
    pub fn ftty_context_resize_render_pipeline(
        ctx: FttyContext,
        handle: FttyPipeline,
        w: u16,
        h: u16,
    ) -> i32;

    pub fn ftty_context_execute_render_pipeline_region(
        ctx: FttyContext,
        handle: FttyPipeline,
        dispatch_x: u16,
        dispatch_y: u16,
        dispatch_w: u16,
        dispatch_h: u16,
    ) -> i32;
    pub fn ftty_context_wait_render_pipeline(ctx: FttyContext, handle: FttyPipeline) -> i32;

    pub fn ftty_pipeline_get_input_surface(handle: FttyPipeline) -> *mut u8;
    pub fn ftty_pipeline_get_output_surface(handle: FttyPipeline) -> *mut FttyUnicodePixel;
    pub fn ftty_pipeline_get_dims(handle: FttyPipeline, w: *mut u16, h: *mut u16);

    pub fn ftty_get_patch_width() -> u8;
    pub fn ftty_get_patch_height() -> u8;
}
