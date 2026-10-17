//! Image files for `Ui::image_file`: decoded on a background thread, watched
//! for changes by modification time, delivered to the context on the UI thread.
use crate::{WakeHandle, hot::Poller};
use seaglass_core::Context;
use std::collections::HashSet;
use std::path::PathBuf;

pub(crate) struct ImageFiles {
    poller: Poller,
    pending: HashSet<PathBuf>,
    #[cfg_attr(not(feature = "images"), allow(dead_code))]
    wake: WakeHandle,
    /// Started by the first image file.
    #[cfg(feature = "images")]
    loader: Option<seaglass_image::Loader>,
}

impl ImageFiles {
    pub fn new(wake: WakeHandle) -> Self {
        Self {
            poller: Poller::default(),
            pending: HashSet::new(),
            wake,
            #[cfg(feature = "images")]
            loader: None,
        }
    }

    /// Hands finished loads to the context before a declaration.
    pub fn deliver(&mut self, context: &mut Context) {
        #[cfg(feature = "images")]
        if let Some(loader) = &self.loader {
            for (path, image) in loader.finished() {
                if let Err(error) = &image {
                    eprintln!("image not loaded: {error}");
                }
                self.pending.remove(&path);
                context.set_image_file(&path, image);
            }
        }
        #[cfg(not(feature = "images"))]
        let _ = context;
    }

    /// Starts loading the images the view asked for: the ones it has never
    /// seen, and with `changed` also the ones whose file changed on disk.
    pub fn request(&mut self, context: &mut Context, changed: bool) {
        for path in context.image_paths().to_vec() {
            let fresh = !context.image_known(&path);
            if self.pending.contains(&path) || !(fresh || changed) {
                continue;
            }
            // Recorded for fresh files too, so the next poll does not reload them.
            if !self.poller.stamp_changed(&path) && !fresh {
                continue;
            }
            #[cfg(feature = "images")]
            {
                let wake = self.wake.clone();
                self.loader
                    .get_or_insert_with(|| {
                        seaglass_image::Loader::new(move || {
                            wake.request_redraw();
                        })
                    })
                    .request(path.clone());
                self.pending.insert(path);
            }
            #[cfg(not(feature = "images"))]
            context.set_image_file(
                &path,
                Err(format!(
                    "{}: built without the `images` feature",
                    path.display()
                )),
            );
        }
    }
}
