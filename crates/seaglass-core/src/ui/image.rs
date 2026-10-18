use super::*;
use crate::{ImageOptions, Length};
use seaglass_scene::Image;

impl<'a, M: 'static> Ui<'a, M> {
    /// A decoded image. It takes its natural size unless the style gives one;
    /// `ImageOptions::fit` decides how it fills a box of another shape.
    #[track_caller]
    pub fn image(&mut self, image: &Image, options: ImageOptions) {
        let key = self.next_key();
        self.image_element(key, Some(image.clone()), false, options);
    }

    /// An image file the host decodes in the background and reloads when it
    /// changes on disk. Until it arrives, and when it cannot be decoded, a
    /// placeholder fills the box, so give the style a size to keep the layout
    /// from jumping. See `Context::image_error` for failures.
    #[track_caller]
    pub fn image_file(&mut self, path: impl AsRef<std::path::Path>, options: ImageOptions) {
        let key = self.next_key();
        let path = path.as_ref();
        if !self.host.images.iter().any(|known| known == path) {
            self.host.images.push(path.to_owned());
        }
        let (image, failed) = self.context.image_file(path);
        self.image_element(key, image, failed, options);
    }

    #[track_caller]
    fn image_element(
        &mut self,
        key: WidgetKey,
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
            text: options.alt,
            style,
            enabled: self.enabled,
            control: Control::Image {
                image,
                failed,
                fit: options.fit,
                tint: options.tint,
            },
            ..Element::new(key, Kind::Image)
        });
    }
}
