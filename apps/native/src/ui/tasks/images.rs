//! The image strip on a task. Ported from `ui/TaskImageStrip.tsx`: each stored
//! image is fetched once through `task.image` and shown as a thumbnail; an image
//! still being staged shows its name, and one that could not be attached shows
//! that instead of vanishing.

use gpui::{div, prelude::*, px, AnyElement, Context};

use crate::state::tasks::{BoardTask, TaskImageRef};
use crate::theme::{self, Colors};
use crate::ui::root::Root;
use crate::ui::tasks::image_chip;

impl Root {
    /// The images of a task the daemon keeps.
    pub(crate) fn render_task_images(&mut self, task: &BoardTask, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let mut strip = div().flex().flex_wrap().gap(px(6.0));
        for image in &task.images {
            self.request_task_image(&task.id, image, cx);
            let decoded = self.task_image(&task.id, image);
            strip = strip.child(image_chip(&image.name, decoded, colors));
        }
        if task.images.is_empty() {
            return div().into_any_element();
        }
        strip.into_any_element()
    }

    /// The images of the form: kept ones from the task, and the ones just added.
    pub(crate) fn render_draft_images(&mut self, task: Option<&BoardTask>, colors: Colors, cx: &mut Context<Self>) -> AnyElement {
        let mut strip = div().flex().flex_wrap().gap(px(6.0));
        let mut count = 0;
        for image in self.tasks.draft.as_ref().map(|draft| draft.images.clone()).unwrap_or_default() {
            match &image {
                TaskImageRef::Stored(id) => {
                    let stored = task.and_then(|task| task.images.iter().find(|image| &image.id == id)).cloned();
                    let (name, decoded) = match (stored, task) {
                        (Some(image), Some(task)) => {
                            self.request_task_image(&task.id, &image, cx);
                            (image.name.clone(), self.task_image(&task.id, &image))
                        }
                        _ => ("image".to_owned(), None),
                    };
                    strip = strip.child(image_chip(&name, decoded, colors));
                }
                TaskImageRef::Staged { name, .. } => strip = strip.child(image_chip(name, None, colors)),
            }
            count += 1;
        }
        if count == 0 {
            return div().text_size(px(theme::FS_2XS)).text_color(colors.text_3).child("No images.").into_any_element();
        }
        strip.into_any_element()
    }
}
