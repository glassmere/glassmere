use super::*;
use crate::{ImageOptions, Length};
use rust_gui_scene::Image;

impl Ui<'_> {
    /// A decoded image. It takes its natural size unless the style gives one;
    /// `ImageOptions::fit` decides how it fills a box of another shape.
    pub fn image(&mut self, key: impl Into<String>, image: &Image, options: ImageOptions) {
        self.image_element(key.into(), Some(image.clone()), false, options);
    }

    /// An image file the host decodes in the background and reloads when it
    /// changes on disk. Until it arrives, and when it cannot be decoded, a
    /// placeholder fills the box, so give the style a size to keep the layout
    /// from jumping. See `Context::image_error` for failures.
    pub fn image_file(
        &mut self,
        key: impl Into<String>,
        path: impl AsRef<std::path::Path>,
        options: ImageOptions,
    ) {
        let path = path.as_ref();
        if !self.host.images.iter().any(|known| known == path) {
            self.host.images.push(path.to_owned());
        }
        let (image, failed) = self.context.image_file(path);
        self.image_element(key.into(), image, failed, options);
    }

    fn image_element(
        &mut self,
        key: String,
        image: Option<Image>,
        failed: bool,
        options: ImageOptions,
    ) {
        let mut style = options.style;
        // One given side: the other follows the image's shape.
        let sides = (style.width != Length::Auto, style.height != Length::Auto);
        if let Some(image) = &image
            && style.aspect_ratio.is_none()
            && sides.0 != sides.1
        {
            let size = image.size();
            if size.width > 0.0 && size.height > 0.0 {
                style.aspect_ratio = Some(size.width / size.height);
            }
        }
        self.push(Element {
            key: WidgetKey::Named(key),
            kind: Kind::Image,
            text: options.alt,
            style,
            enabled: self.enabled,
            children: Vec::new(),
            custom: None,
            tooltip: None,
            control: Control::Image {
                image,
                failed,
                fit: options.fit,
                tint: options.tint,
            },
        });
    }
}
