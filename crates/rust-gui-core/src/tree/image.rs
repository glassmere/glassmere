use super::*;
use crate::{ImageSource, ImageTint};
use rust_gui_scene::Image;
use std::path::{Path, PathBuf};

/// An image file the host decodes for `Ui::image_file`.
#[derive(Default)]
pub(super) struct LoadedImage {
    image: Option<Image>,
    error: Option<String>,
}

impl LoadedImage {
    /// The decoded pixels, when it is a raster image that loaded.
    pub(super) fn pixels(&self) -> Option<(u32, u32, &[u8])> {
        self.image.as_ref()?.pixels()
    }
}

/// Decoded pixels kept for files no declaration asks for any more, so that
/// switching back to a view does not decode again. Past this they are dropped.
const UNUSED_IMAGE_BYTES: usize = 64 << 20;

impl Context {
    /// Image files the last declaration asked for through `Ui::image_file`.
    /// Hosts decode them off the UI thread, watch them for changes and report
    /// through `set_image_file`.
    pub fn image_paths(&self) -> &[PathBuf] {
        &self.image_requests
    }

    /// Whether the host has delivered this image (or an error for it) yet.
    pub fn image_known(&self, path: &Path) -> bool {
        self.images.contains_key(path)
    }

    /// Delivers a decoded image file or the reason it could not be loaded. An
    /// error keeps the last good image. Returns whether a declaration should
    /// follow.
    pub fn set_image_file(
        &mut self,
        path: &Path,
        image: std::result::Result<Image, String>,
    ) -> bool {
        let entry = self.images.entry(path.to_owned()).or_default();
        match image {
            Ok(image) => {
                entry.image = Some(image);
                entry.error = None;
            }
            Err(error) => {
                if entry.error.as_deref() == Some(&error) {
                    return false;
                }
                entry.error = Some(error);
            }
        }
        self.pending |= Dirty::PAINT;
        true
    }

    /// Why the last load of an image file failed, if it did.
    pub fn image_error(&self, path: &Path) -> Option<&str> {
        self.images.get(path)?.error.as_deref()
    }

    /// The image to show and whether there is none because loading failed.
    pub(crate) fn image_file(&self, path: &Path) -> (Option<Image>, bool) {
        self.images.get(path).map_or((None, false), |loaded| {
            (
                loaded.image.clone(),
                loaded.image.is_none() && loaded.error.is_some(),
            )
        })
    }

    /// After a declaration: forget images nothing shows once they add up.
    pub(super) fn trim_images(&mut self) {
        let unused = |(path, _): &(&PathBuf, &LoadedImage)| !self.image_requests.contains(path);
        let bytes: usize = self
            .images
            .iter()
            .filter(unused)
            .filter_map(|(_, loaded)| loaded.image.as_ref()?.pixels())
            .map(|(_, _, pixels)| pixels.len())
            .sum();
        if bytes > UNUSED_IMAGE_BYTES {
            let requests = &self.image_requests;
            self.images.retain(|path, _| requests.contains(path));
        }
    }

    /// `Style::background_image`, over the background colour.
    pub(super) fn paint_background_image(
        &self,
        node: &Node,
        scene: &mut Scene,
        foreground: Color,
        radius: f32,
    ) {
        let Some(background) = &node.style.background_image else {
            return;
        };
        // A file that has not arrived yet paints nothing.
        let ImageSource::Image(image) = &background.source else {
            return;
        };
        let tint = match background.tint {
            ImageTint::None => None,
            ImageTint::Foreground => Some(foreground),
            ImageTint::Color(color) => Some(color),
        };
        let (pieces, radius) = match background.slice {
            Some(slice) => (
                crate::image::nine_slice(slice, image.size(), node.bounds),
                0.0,
            ),
            None => (
                background
                    .fit
                    .place(image.size(), node.bounds)
                    .into_iter()
                    .collect(),
                radius,
            ),
        };
        for (rect, source) in pieces {
            scene.primitives.push(Primitive::Image {
                rect,
                image: image.clone(),
                source,
                tint,
                radius,
            });
        }
    }

    pub(super) fn paint_image(
        &self,
        id: NodeId,
        scene: &mut Scene,
        foreground: Color,
        radius: f32,
    ) {
        let node = &self.nodes[&id];
        let Control::Image {
            image,
            failed,
            fit,
            tint,
        } = &node.control
        else {
            return;
        };
        let padding = node.style.padding.max(0.0);
        let content = Rect {
            x: node.bounds.x + padding,
            y: node.bounds.y + padding,
            width: (node.bounds.width - padding * 2.0).max(0.0),
            height: (node.bounds.height - padding * 2.0).max(0.0),
        };
        let Some(image) = image else {
            // Still loading, or failed: a quiet box, with the name if loading failed.
            scene.primitives.push(Primitive::Quad {
                rect: node.bounds,
                color: self.theme.track,
                radius,
            });
            if *failed && !node.text.is_empty() {
                scene.primitives.push(Primitive::StyledText {
                    rect: content,
                    text: node.text.clone(),
                    style: node.text_style(),
                    color: foreground,
                });
            }
            return;
        };
        if let Some((rect, source)) = fit.place(image.size(), content) {
            scene.primitives.push(Primitive::Image {
                rect,
                image: image.clone(),
                source,
                tint: match tint {
                    ImageTint::None => None,
                    ImageTint::Foreground => Some(foreground),
                    ImageTint::Color(color) => Some(*color),
                },
                radius,
            });
        }
    }
}
