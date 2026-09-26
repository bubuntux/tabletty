//! Drawing a game's SVG art as a terminal image.
//!
//! The SVG is rasterised with resvg at the pixel size of the cells it will occupy,
//! then handed to ratatui-image, which uses whatever the terminal supports: Kitty,
//! iTerm2 or Sixel graphics, or coloured half-blocks. Each (key, size) is rasterised
//! once and cached.

use std::cell::RefCell;
use std::collections::HashMap;

use image::{DynamicImage, RgbaImage};
use ratatui::Frame;
use ratatui::layout::{Rect, Size};
use ratatui_image::picker::{Picker, ProtocolType};
use ratatui_image::protocol::Protocol;
use ratatui_image::{Image, Resize};
use resvg::{tiny_skia, usvg};

/// Upper bound on a rendered side, whatever the terminal's cell size claims.
const MAX_PIXELS_PER_SIDE: u32 = 1024;

pub struct Pictures {
    /// `None` when images are off: everything falls back to glyphs and labels.
    picker: Option<Picker>,
    /// `None` entries remember art that failed to render, so it isn't retried.
    cache: RefCell<HashMap<(String, u16, u16), Option<Protocol>>>,
}

impl Pictures {
    pub fn new(picker: Option<Picker>) -> Self {
        Self {
            picker,
            cache: RefCell::new(HashMap::new()),
        }
    }

    /// Whether images can be drawn at all.
    pub fn enabled(&self) -> bool {
        self.picker.is_some()
    }

    /// Whether an image would beat a glyph in an area this size. Half-blocks give
    /// only two pixels per cell, so small areas look better as text.
    pub fn worth_drawing(&self, area: Rect) -> bool {
        match self.picker.as_ref().map(Picker::protocol_type) {
            None => false,
            Some(ProtocolType::Halfblocks) => area.width >= 8 && area.height >= 4,
            Some(_) => area.width >= 4 && area.height >= 2,
        }
    }

    /// Draw `svg` into `area`. Returns false if it couldn't be, so the caller can
    /// fall back to text.
    pub fn draw(&self, frame: &mut Frame, area: Rect, key: &str, svg: &str) -> bool {
        let Some(picker) = &self.picker else {
            return false;
        };
        let mut cache = self.cache.borrow_mut();
        let protocol = cache
            .entry((key.to_string(), area.width, area.height))
            .or_insert_with(|| match rasterize(picker, svg, area) {
                Ok(protocol) => Some(protocol),
                Err(err) => {
                    tracing::warn!(key, "could not draw art: {err}");
                    None
                }
            });
        match protocol {
            Some(protocol) => {
                frame.render_widget(Image::new(protocol), area);
                true
            }
            None => false,
        }
    }
}

/// Parsing options for plugin SVG. `<image href>` is never loaded: usvg's default
/// resolver reads paths from disk, and this SVG comes from a plugin.
fn svg_options() -> usvg::Options<'static> {
    usvg::Options {
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

fn rasterize(picker: &Picker, svg: &str, area: Rect) -> Result<Protocol, String> {
    let tree = usvg::Tree::from_str(svg, &svg_options()).map_err(|err| err.to_string())?;

    let font = picker.font_size();
    let width = (u32::from(area.width) * u32::from(font.width)).min(MAX_PIXELS_PER_SIDE);
    let height = (u32::from(area.height) * u32::from(font.height)).min(MAX_PIXELS_PER_SIDE);
    let mut pixmap = tiny_skia::Pixmap::new(width, height).ok_or("zero-sized area")?;

    // Fit the art inside the area, centred, keeping its aspect ratio.
    let art = tree.size();
    let scale = (width as f32 / art.width()).min(height as f32 / art.height());
    let offset_x = (width as f32 - art.width() * scale) / 2.0;
    let offset_y = (height as f32 - art.height() * scale) / 2.0;
    let transform = tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, offset_x, offset_y);
    resvg::render(&tree, transform, &mut pixmap.as_mut());

    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|pixel| {
            let color = pixel.demultiply();
            [color.red(), color.green(), color.blue(), color.alpha()]
        })
        .collect();
    let image = RgbaImage::from_raw(width, height, rgba).ok_or("pixel buffer size mismatch")?;
    picker
        .new_protocol(
            DynamicImage::ImageRgba8(image),
            Size::new(area.width, area.height),
            Resize::Fit(None),
        )
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    const SQUARE: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
        <rect width="10" height="10" fill="#c00"/></svg>"##;

    #[test]
    fn svg_rasterises_through_halfblocks() {
        let area = Rect::new(0, 0, 8, 4);
        assert!(rasterize(&Picker::halfblocks(), SQUARE, area).is_ok());
    }

    #[test]
    fn external_images_are_never_resolved() {
        let options = svg_options();
        let resolver = &options.image_href_resolver;
        assert!((resolver.resolve_string)("/etc/passwd", &options).is_none());
        let data = std::sync::Arc::new(SQUARE.as_bytes().to_vec());
        assert!((resolver.resolve_data)("image/svg+xml", data, &options).is_none());
    }

    #[test]
    fn garbage_is_an_error_not_a_panic() {
        assert!(rasterize(&Picker::halfblocks(), "<not svg", Rect::new(0, 0, 8, 4)).is_err());
    }
}
