//! What a copy carries.
//!
//! A clipboard holds one thing in several forms at once, which is why this is
//! a struct rather than a choice: a copied shape is bytes to this application
//! and a line of text to everything else, and both go on together.

use glassmere_scene::Image;

/// One copy, in every form it has.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Clip {
    /// What other applications see. Empty means the copy says nothing to
    /// them, which is a surprise worth avoiding.
    pub text: String,
    /// Pixels, when there are some.
    pub image: Option<Image>,
    /// Bytes under a name of the application's own, and that name. These
    /// stay inside this application: putting a private format on the system
    /// clipboard needs platform calls the framework does not make. They
    /// survive between its windows, and are dropped as soon as anything else
    /// takes the clipboard over.
    pub custom: Option<(String, Vec<u8>)>,
}

impl Clip {
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            ..Self::default()
        }
    }

    pub fn image(image: Image) -> Self {
        Self {
            image: Some(image),
            ..Self::default()
        }
    }

    /// Bytes only this application understands. Give it a text form too,
    /// with [`Clip::with_text`], unless nothing else should see anything.
    pub fn custom(format: impl Into<String>, bytes: Vec<u8>) -> Self {
        Self {
            custom: Some((format.into(), bytes)),
            ..Self::default()
        }
    }

    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = text.into();
        self
    }

    pub fn with_image(mut self, image: Image) -> Self {
        self.image = Some(image);
        self
    }

    /// The bytes of `format`, when that is what this clip carries.
    pub fn bytes(&self, format: &str) -> Option<&[u8]> {
        self.custom
            .as_ref()
            .filter(|(name, _)| name == format)
            .map(|(_, bytes)| bytes.as_slice())
    }

    pub fn is_empty(&self) -> bool {
        self.text.is_empty() && self.image.is_none() && self.custom.is_none()
    }
}
