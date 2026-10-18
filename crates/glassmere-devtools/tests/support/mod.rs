//! The inspector the way a host runs it: an application whose message is
//! `InspectorMsg`, declared every frame because the model is replaced before
//! each one.
#![allow(dead_code)]

use glassmere_core::{App, AppDriver, Context, Effects, MonospaceMeasurer, Ui};
use glassmere_devtools::{Inspector, InspectorModel, InspectorMsg, InspectorResponse};
use glassmere_scene::Size;

pub const VIEWPORT: Size = Size::new(800.0, 700.0);

pub struct Host {
    pub inspector: Inspector,
    pub model: InspectorModel,
    response: InspectorResponse,
}

impl App for Host {
    type Message = InspectorMsg;

    fn update(&mut self, message: InspectorMsg, _: &mut Effects<'_, InspectorMsg>) {
        let response = self.inspector.update(message, &self.model);
        self.response.export_csv |= response.export_csv;
        self.response.focus = response.focus.or(self.response.focus);
    }

    fn view(&self, ui: &mut Ui<'_, InspectorMsg>) {
        self.inspector.view(ui, &self.model);
    }
}

pub struct Shown {
    pub host: Host,
    pub context: Context,
    driver: AppDriver<InspectorMsg>,
}

impl Shown {
    pub fn new(inspector: Inspector, mut model: InspectorModel) -> Self {
        model.viewport = VIEWPORT;
        Self {
            host: Host {
                inspector,
                model,
                response: InspectorResponse::default(),
            },
            context: Context::new(),
            driver: AppDriver::new(|| true),
        }
    }

    /// One frame, and what the inspector asked of the inspected window in it.
    pub fn frame(&mut self) -> InspectorResponse {
        self.host.inspector.sync(&self.host.model);
        self.driver.invalidate();
        self.driver.frame(
            &mut self.host,
            &mut self.context,
            VIEWPORT,
            &mut MonospaceMeasurer,
        );
        std::mem::take(&mut self.host.response)
    }
}
