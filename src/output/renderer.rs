use std::{
    io::{self, Write},
    ptr,
    rc::Rc,
    time::Instant,
};

use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthStr;

use crate::{
    gfx::{Color, Point, Rect, Size},
    input::Key,
    ui::navigation::{Navigation, NavigationAction},
    utils::log,
};

use super::{binarize_quandrant, Cell, Grapheme, Painter};
use super::fidelitty::*;

struct FidelittyState {
    ctx: FttyContext,
    pipeline: FttyPipeline,
}

pub struct Renderer {
    nav: Navigation,
    cells: Vec<(Cell, Cell)>,
    painter: Painter,
    size: Size,
    ftty: Option<FidelittyState>,
}

impl Renderer {
    pub fn new() -> Renderer {
        let ftty_ctx = unsafe { ftty_context_create(1) };
        let ftty = if ftty_ctx.is_null() {
            log::debug!("fidelitty: Vulkan not available, using fallback renderer");
            None
        } else {
            log::debug!("fidelitty: Vulkan context created");
            Some(FidelittyState {
                ctx: ftty_ctx,
                pipeline: ptr::null_mut(),
            })
        };

        Renderer {
            nav: Navigation::new(),
            cells: Vec::with_capacity(0),
            painter: Painter::new(),
            size: Size::new(0, 0),
            ftty,
        }
    }

    pub fn enable_true_color(&mut self) {
        self.painter.set_true_color(true)
    }

    pub fn keypress(&mut self, key: &Key) -> io::Result<NavigationAction> {
        let action = self.nav.keypress(key);

        Ok(action)
    }
    pub fn mouse_up(&mut self, origin: Point) -> io::Result<NavigationAction> {
        let action = self.nav.mouse_up(origin);

        Ok(action)
    }
    pub fn mouse_down(&mut self, origin: Point) -> io::Result<NavigationAction> {
        let action = self.nav.mouse_down(origin);

        Ok(action)
    }
    pub fn mouse_move(&mut self, origin: Point) -> io::Result<NavigationAction> {
        let action = self.nav.mouse_move(origin);

        Ok(action)
    }

    pub fn push_nav(&mut self, url: &str, can_go_back: bool, can_go_forward: bool) {
        self.nav.push(url, can_go_back, can_go_forward)
    }

    pub fn get_size(&self) -> Size {
        self.size
    }

    pub fn set_size(&mut self, size: Size) {
        self.nav.set_size(size);
        self.size = size;

        let mut x = 0;
        let mut y = 0;
        let bound = size.width - 1;
        let cells = (size.width + size.width * size.height) as usize;

        self.cells.clear();
        self.cells.resize_with(cells, || {
            let cell = (Cell::new(x, y), Cell::new(x, y));

            if x < bound {
                x += 1;
            } else {
                x = 0;
                y += 1;
            }

            cell
        });

        // Create or resize fidelitty pipeline
        if let Some(ref mut ftty) = self.ftty {
            let w = size.width as u16;
            let h = size.height as u16;

            unsafe {
                if ftty.pipeline.is_null() {
                    ftty.pipeline = ftty_context_create_render_pipeline_ex(
                        ftty.ctx, w, h,
                        FttyPixelFormat::Bgra, 2, 4,
                    );
                    if ftty.pipeline.is_null() {
                        log::debug!("fidelitty: failed to create render pipeline");
                    }
                } else {
                    let ret = ftty_context_resize_render_pipeline(
                        ftty.ctx, ftty.pipeline, w, h,
                    );
                    if ret != 0 {
                        log::debug!("fidelitty: failed to resize render pipeline");
                    }
                }
            }
        }
    }

    pub fn render(&mut self) -> io::Result<()> {
        let t_start = Instant::now();
        let size = self.size;

        for (origin, element) in self.nav.render(size) {
            self.fill_rect(
                Rect::new(origin.x, origin.y, element.text.width() as u32, 1),
                element.background,
            );
            self.draw_text(
                &element.text,
                origin * (2, 1),
                Size::splat(0),
                element.foreground,
            );
        }

        let t_nav = t_start.elapsed();

        self.painter.begin()?;

        let mut cells_painted = 0u32;
        for (previous, current) in self.cells.iter_mut() {
            if current == previous {
                continue;
            }

            previous.quadrant = current.quadrant;
            previous.background = current.background;
            previous.foreground = current.foreground;
            previous.codepoint = current.codepoint;
            previous.grapheme = current.grapheme.clone();

            self.painter.paint(current)?;
            cells_painted += 1;
        }

        let t_diff_paint = t_start.elapsed();

        self.painter.end(self.nav.cursor())?;

        let t_flush = t_start.elapsed();

        log::debug!(
            "render: {} cells painted | nav: {:?} | diff+paint: {:?} | flush: {:?} | total: {:?}",
            cells_painted,
            t_nav,
            t_diff_paint - t_nav,
            t_flush - t_diff_paint,
            t_flush
        );

        Ok(())
    }

    /// Draw the background from a pixel array encoded in BGRA8888
    pub fn draw_background(&mut self, pixels: &[u8], pixels_size: Size, rect: Rect) {
        let viewport = self.size.cast::<usize>();

        if pixels.len() < viewport.width * viewport.height * 8 * 4 {
            log::debug!(
                "unexpected size, actual: {}, expected: {}",
                pixels.len(),
                viewport.width * viewport.height * 8 * 4
            );
            return;
        }

        let origin = rect.origin.cast::<f32>().max(0.0) / (2.0, 4.0);
        let size = rect.size.cast::<f32>().max(0.0) / (2.0, 4.0);
        let top = (origin.y.floor() as usize).min(viewport.height);
        let left = (origin.x.floor() as usize).min(viewport.width);
        let right = ((origin.x + size.width).ceil() as usize)
            .min(viewport.width)
            .max(left);
        let bottom = ((origin.y + size.height).ceil() as usize)
            .min(viewport.height)
            .max(top);

        let use_fidelitty = match self.ftty {
            Some(ref ftty) => !ftty.pipeline.is_null(),
            None => false,
        };

        if use_fidelitty {
            self.draw_background_fidelitty(pixels, pixels_size, top, left, right, bottom);
        } else {
            self.draw_background_fallback(pixels, pixels_size, top, left, right, bottom);
        }
    }

    fn draw_background_fidelitty(
        &mut self,
        pixels: &[u8],
        pixels_size: Size,
        top: usize,
        left: usize,
        right: usize,
        bottom: usize,
    ) {
        let t_start = Instant::now();

        let ftty = self.ftty.as_ref().unwrap();
        let viewport = self.size.cast::<usize>();
        let row_length = pixels_size.width as usize;
        let input_width = viewport.width * 2; // 2 pixels per cell, BGRA

        let input_surface = unsafe { ftty_pipeline_get_input_surface(ftty.pipeline) };
        if input_surface.is_null() {
            return;
        }

        let dispatch_w = (right - left) as u16;
        let dispatch_h = (bottom - top) as u16;

        if dispatch_w == 0 || dispatch_h == 0 {
            return;
        }

        // Copy BGRA pixels directly to fidelitty input surface.
        // The GPU shader handles BGRA→RGB swizzle and 2→4 horizontal upscale.
        let bpp = 4usize; // BGRA bytes per pixel
        let row_bytes = input_width * bpp;

        if row_length == input_width {
            // Strides match — single memcpy for the entire row block
            let start = top * 4 * row_bytes;
            let total = (bottom - top) * 4 * row_bytes;
            unsafe {
                ptr::copy_nonoverlapping(
                    pixels.as_ptr().add(start),
                    input_surface.add(start),
                    total,
                );
            }
        } else {
            // Different strides — copy row by row
            let copy_bytes = (right - left) * 2 * bpp;
            for py in (top * 4)..(bottom * 4) {
                let src_offset = (py * row_length + left * 2) * bpp;
                let dst_offset = (py * input_width + left * 2) * bpp;
                unsafe {
                    ptr::copy_nonoverlapping(
                        pixels.as_ptr().add(src_offset),
                        input_surface.add(dst_offset),
                        copy_bytes,
                    );
                }
            }
        }

        let t_copy = t_start.elapsed();

        unsafe {
            let ret = ftty_context_execute_render_pipeline_region(
                ftty.ctx,
                ftty.pipeline,
                left as u16,
                top as u16,
                dispatch_w,
                dispatch_h,
            );
            if ret != 0 {
                log::debug!("fidelitty: execute failed");
                return;
            }

            let ret = ftty_context_wait_render_pipeline(ftty.ctx, ftty.pipeline);
            if ret != 0 {
                log::debug!("fidelitty: wait failed");
                return;
            }
        }

        let t_gpu = t_start.elapsed();

        // Read fidelitty output into cells
        let output_surface = unsafe { ftty_pipeline_get_output_surface(ftty.pipeline) };
        if output_surface.is_null() {
            return;
        }

        for y in top..bottom {
            let cell_index = (y + 1) * viewport.width;
            let out_row = y * viewport.width;

            for cx in left..right {
                let ftty_pixel = unsafe { *output_surface.add(out_row + cx) };
                let cell = &mut self.cells[cell_index + cx].1;

                cell.background = Color::new(ftty_pixel.br, ftty_pixel.bg, ftty_pixel.bb);
                cell.foreground = Color::new(ftty_pixel.fr, ftty_pixel.fg, ftty_pixel.fb);
                cell.codepoint = ftty_pixel.codepoint;
            }
        }

        let t_readback = t_start.elapsed();

        log::debug!(
            "fidelitty: {}x{} region | copy: {:?} | gpu: {:?} | readback: {:?} | total: {:?}",
            dispatch_w,
            dispatch_h,
            t_copy,
            t_gpu - t_copy,
            t_readback - t_gpu,
            t_readback
        );
    }

    fn draw_background_fallback(
        &mut self,
        pixels: &[u8],
        pixels_size: Size,
        top: usize,
        left: usize,
        right: usize,
        bottom: usize,
    ) {
        let viewport = self.size.cast::<usize>();
        let row_length = pixels_size.width as usize;
        let pixel = |x: usize, y: usize| {
            Color::new(
                pixels[(x + y * row_length) * 4 + 2],
                pixels[(x + y * row_length) * 4 + 1],
                pixels[(x + y * row_length) * 4 + 0],
            )
        };
        let pair = |x: usize, y: usize| pixel(x, y).avg_with(pixel(x, y + 1));

        for y in top..bottom {
            let index = (y + 1) * viewport.width;
            let start = index + left;
            let end = index + right;
            let (mut x, y) = (left * 2, y * 4);

            for (_, cell) in &mut self.cells[start..end] {
                let quadrant = (
                    pair(x + 0, y + 0),
                    pair(x + 1, y + 0),
                    pair(x + 1, y + 2),
                    pair(x + 0, y + 2),
                );

                cell.quadrant = quadrant;
                let (ch, bg, fg) = binarize_quandrant(quadrant);
                cell.background = bg;
                cell.foreground = fg;
                cell.codepoint = ch.chars().next().unwrap_or(' ') as u32;

                x += 2;
            }
        }
    }

    pub fn clear_text(&mut self) {
        for (_, cell) in self.cells.iter_mut() {
            cell.grapheme = None
        }
    }

    pub fn set_title(&self, title: &str) -> io::Result<()> {
        let mut stdout = io::stdout();

        write!(stdout, "\x1b]0;{title}\x07")?;
        write!(stdout, "\x1b]1;{title}\x07")?;
        write!(stdout, "\x1b]2;{title}\x07")?;

        stdout.flush()
    }

    pub fn fill_rect(&mut self, rect: Rect, color: Color) {
        self.draw(rect, |cell| {
            cell.grapheme = None;
            cell.quadrant = (color, color, color, color);
            cell.background = color;
            cell.foreground = color;
            cell.codepoint = 0x20; // space
        })
    }

    pub fn draw<F>(&mut self, bounds: Rect, mut draw: F)
    where
        F: FnMut(&mut Cell),
    {
        let origin = bounds.origin.cast::<usize>();
        let size = bounds.size.cast::<usize>();
        let viewport_width = self.size.width as usize;
        let top = origin.y;
        let bottom = top + size.height;

        // Iterate over each row
        for y in top..bottom {
            let left = y * viewport_width + origin.x;
            let right = left + size.width;

            for (_, current) in self.cells[left..right].iter_mut() {
                draw(current)
            }
        }
    }

    /// Render some text into the terminal output
    pub fn draw_text(&mut self, string: &str, origin: Point, size: Size, color: Color) {
        // Get an iterator starting at the text origin
        let len = self.cells.len();
        let viewport = &self.size.cast::<usize>();

        if size.width > 2 && size.height > 2 {
            let origin = (origin.cast::<f32>() / (2.0, 4.0) + (0.0, 1.0)).round();
            let size = (size.cast::<f32>() / (2.0, 4.0)).round();
            let left = (origin.x.max(0.0) as usize).min(viewport.width);
            let right = ((origin.x + size.width).max(0.0) as usize).min(viewport.width);
            let top = (origin.y.max(0.0) as usize).min(viewport.height);
            let bottom = ((origin.y + size.height).max(0.0) as usize).min(viewport.height);

            for y in top..bottom {
                let index = y * viewport.width;
                let start = index + left;
                let end = index + right;

                for (_, cell) in self.cells[start..end].iter_mut() {
                    cell.grapheme = None
                }
            }
        } else {
            // Compute the buffer index based on the position
            let index = origin.x / 2 + (origin.y + 1) / 4 * (viewport.width as i32);
            let mut iter = self.cells[len.min(index as usize)..].iter_mut();

            // Get every Unicode grapheme in the input string
            for grapheme in UnicodeSegmentation::graphemes(string, true) {
                let width = grapheme.width();

                for index in 0..width {
                    // Get the next terminal cell at the given position
                    match iter.next() {
                        // Stop if we're at the end of the buffer
                        None => return,
                        // Set the cell to the current grapheme
                        Some((_, cell)) => {
                            let next = Grapheme {
                                // Create a new shared reference to the text
                                color,
                                index,
                                width,
                                // Export the set of unicode code points for this graphene into an UTF-8 string
                                char: grapheme.to_string(),
                            };

                            if match cell.grapheme {
                                None => true,
                                Some(ref previous) => {
                                    previous.color != next.color || previous.char != next.char
                                }
                            } {
                                cell.grapheme = Some(Rc::new(next))
                            }
                        }
                    }
                }
            }
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        if let Some(ref mut ftty) = self.ftty {
            unsafe {
                if !ftty.pipeline.is_null() {
                    ftty_context_destroy_render_pipeline(ftty.ctx, ftty.pipeline);
                }
                ftty_context_destroy(ftty.ctx);
            }
        }
    }
}
