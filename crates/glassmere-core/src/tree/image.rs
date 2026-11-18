use super::*;
use crate::{ImageSource, ImageTint};
use alloc::vec::Vec;
#[cfg(feature = "std")]
use alloc::{borrow::ToOwned, string::String};
use glassmere_scene::Image;
#[cfg(feature = "std")]
use std::path::{Path, PathBuf};

/// An image file the host decodes for `Ui::image_file`.
#[cfg(feature = "std")]
#[derive(Default)]
pub(super) struct LoadedImage {
    image: Option<Image>,
    error: Option<String>,
}

#[cfg(feature = "std")]
impl LoadedImage {
    /// The decoded pixels, when it is a raster image that loaded.
    pub(super) fn pixels(&self) -> Option<(u32, u32, &[u8])> {
        self.image.as_ref()?.pixels()
    }
}

/// Decoded pixels kept for files no declaration asks for any more, so that
/// switching back to a view does not decode again. Past this they are dropped.
#[cfg(feature = "std")]
const UNUSED_IMAGE_BYTES: usize = 64 << 20;

/// An animated image a node shows: which one, when it started, and when its
/// frame next changes (`None` once it rests on its last frame, and under
/// reduced motion).
pub(super) struct Play {
    image: u64,
    start: Duration,
    next: Option<Duration>,
}

/// Decoded bytes an image holds, all frames of an animation included.
#[cfg(feature = "std")]
fn pixel_bytes(image: &Image) -> usize {
    let bytes = |image: &Image| image.pixels().map_or(0, |(_, _, pixels)| pixels.len());
    match image.frames() {
        [] => bytes(image),
        frames => frames.iter().map(|frame| bytes(&frame.image)).sum(),
    }
}

/// The frame of an animated image showing `elapsed` after it started, and when
/// after the start the next one comes; `None` while it rests on the last frame
/// of its last loop.
fn frame_at(image: &Image, elapsed: Duration) -> (usize, Option<Duration>) {
    let frames = image.frames();
    let last = frames.len() - 1;
    let lap: Duration = frames.iter().map(|frame| frame.delay).sum();
    let laps = elapsed.as_nanos() / lap.as_nanos();
    let loops = image.loops().map(|loops| u128::from(loops.get()));
    if loops.is_some_and(|loops| laps >= loops) {
        return (last, None);
    }
    let into = Duration::from_nanos((elapsed.as_nanos() % lap.as_nanos()) as u64);
    let mut end = Duration::ZERO;
    let index = frames
        .iter()
        .position(|frame| {
            end += frame.delay;
            into < end
        })
        .unwrap_or(last);
    let resting = index == last && loops.is_some_and(|loops| laps + 1 == loops);
    (index, (!resting).then_some(elapsed - into + end))
}

impl Context {
    /// Image files the last declaration asked for through `Ui::image_file`.
    /// Hosts decode them off the UI thread, watch them for changes and report
    /// through `set_image_file`.
    #[cfg(feature = "std")]
    pub fn image_paths(&self) -> &[PathBuf] {
        &self.image_requests
    }

    /// Whether the host has delivered this image (or an error for it) yet.
    #[cfg(feature = "std")]
    pub fn image_known(&self, path: &Path) -> bool {
        self.images.contains_key(path)
    }

    /// Delivers a decoded image file or the reason it could not be loaded. An
    /// error keeps the last good image. Returns whether a declaration should
    /// follow.
    #[cfg(feature = "std")]
    pub fn set_image_file(
        &mut self,
        path: &Path,
        image: core::result::Result<Image, String>,
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
                self.diagnose_resource(path, &error);
                let entry = self.images.entry(path.to_owned()).or_default();
                entry.error = Some(error);
            }
        }
        self.pending |= Dirty::PAINT;
        // A background image is resolved while declaring.
        self.view_stale = true;
        true
    }

    /// Why the last load of an image file failed, if it did.
    #[cfg(feature = "std")]
    pub fn image_error(&self, path: &Path) -> Option<&str> {
        self.images.get(path)?.error.as_deref()
    }

    /// The image to show and whether there is none because loading failed.
    #[cfg(feature = "std")]
    pub(crate) fn image_file(&self, path: &Path) -> (Option<Image>, bool) {
        self.images.get(path).map_or((None, false), |loaded| {
            (
                loaded.image.clone(),
                loaded.image.is_none() && loaded.error.is_some(),
            )
        })
    }

    /// The frame an animated image shows in its node now, which starts
    /// playing on the first call; `None` for a node without an animated image.
    pub(super) fn image_frame(&mut self, id: NodeId) -> Option<Image> {
        let node = self
            .nodes
            .get(&id)
            .filter(|node| node.kind == Kind::Image)?;
        let Control::Image {
            image: Some(image), ..
        } = &node.control
        else {
            self.image_plays.remove(&id);
            return None;
        };
        if image.frames().is_empty() {
            self.image_plays.remove(&id);
            return None;
        }
        let now = self.now;
        let fresh = || Play {
            image: image.id(),
            start: now,
            next: None,
        };
        let play = self.image_plays.entry(id).or_insert_with(fresh);
        if play.image != image.id() {
            *play = fresh();
        }
        let (index, next) = if self.reduced_motion {
            (0, None)
        } else {
            frame_at(image, now.saturating_sub(play.start))
        };
        play.next = next.map(|after| play.start + after);
        Some(image.frames()[index].image.clone())
    }

    /// Whether any of the node shows; animations off screen do not run.
    fn image_visible(&self, id: NodeId) -> bool {
        self.nodes
            .get(&id)
            .is_some_and(|node| node.clip.width > 0.0 && node.clip.height > 0.0)
    }

    /// When the next animated image on screen changes frame.
    pub(super) fn image_wake(&self) -> Option<Duration> {
        self.image_plays
            .iter()
            .filter(|(id, _)| self.image_visible(**id))
            .filter_map(|(_, play)| play.next)
            .min()
    }

    /// Before a frame: repaints, within their own bounds, the animated images
    /// whose frame is due to change.
    pub(super) fn update_images(&mut self) {
        let now = self.now;
        self.image_plays.retain(|id, _| self.nodes.contains_key(id));
        let due: Vec<NodeId> = self
            .image_plays
            .iter()
            .filter(|(id, play)| {
                play.next.is_some_and(|next| next <= now) && self.image_visible(**id)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in due {
            self.invalidate_paint(id);
        }
    }

    /// After a declaration: forget images nothing shows once they add up.
    #[cfg(feature = "std")]
    pub(super) fn trim_images(&mut self) {
        let unused = |(path, _): &(&PathBuf, &LoadedImage)| !self.image_requests.contains(path);
        let bytes: usize = self
            .images
            .iter()
            .filter(unused)
            .filter_map(|(_, loaded)| loaded.image.as_ref())
            .map(pixel_bytes)
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
        #[cfg(feature = "std")]
        let ImageSource::Image(image) = &background.source else {
            return;
        };
        #[cfg(not(feature = "std"))]
        let ImageSource::Image(image) = &background.source;
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
        frame: Option<Image>,
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
        let content = node.content_box();
        let Some(image) = frame.as_ref().or(image.as_ref()) else {
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
