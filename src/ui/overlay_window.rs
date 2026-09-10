//! Keep allocation stable across ABC collapse to avoid resize-driven input changes.
//! Clip inactive rows from painting and pointer/touch input.
use relm4::gtk::{self, glib, prelude::*, subclass::prelude::*};
use std::cell::{Cell, RefCell};

#[derive(Default)]
pub struct Inner {
    marker: RefCell<Option<(glib::WeakRef<gtk::Widget>, glib::WeakRef<gtk::Widget>)>>,
    top: Cell<f32>,
}
#[glib::object_subclass]
impl ObjectSubclass for Inner {
    const NAME: &'static str = "NovaKeysOverlayWindow";
    type Type = Window;
    type ParentType = gtk::Window;
}
impl ObjectImpl for Inner {}
impl WindowImpl for Inner {}
impl WidgetImpl for Inner {
    fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
        self.parent_size_allocate(width, height, baseline);
        self.obj().update_region();
    }
    fn map(&self) {
        self.parent_map();
        self.obj().update_region();
    }
    fn unmap(&self) {
        if let Some(surface) = self.obj().surface() {
            surface.set_input_region(None);
        }
        self.parent_unmap();
    }
    fn snapshot(&self, snapshot: &gtk::Snapshot) {
        let object = self.obj();
        let top = self.top.get();
        snapshot.push_clip(&gtk::graphene::Rect::new(
            0.0,
            top,
            object.width() as f32,
            (object.height() as f32 - top).max(0.0),
        ));
        self.parent_snapshot(snapshot);
        snapshot.pop();
    }
}
glib::wrapper! {
    pub struct Window(ObjectSubclass<Inner>)
        @extends gtk::Window, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget,
            gtk::Native, gtk::Root, gtk::ShortcutManager;
}
impl Window {
    pub fn new() -> Self {
        let window: Self = glib::Object::builder()
            .property("title", "NOVA Keys")
            .property("decorated", false)
            .build();
        window.add_css_class("novakeys-overlay");
        window
    }
    fn update_region(&self) {
        let top = self
            .imp()
            .marker
            .borrow()
            .as_ref()
            .and_then(|(marker, decoration)| Some((marker.upgrade()?, decoration.upgrade()?)))
            .and_then(|(marker, decoration)| {
                let bounds = marker.compute_bounds(self)?;
                let outer = decoration.compute_bounds(self)?;
                let content = decoration.first_child()?.compute_bounds(self)?;
                let inset = (content.y() - outer.y()).max(0.0);
                Some((bounds.y() - inset).max(0.0))
            })
            .unwrap_or(0.0)
            .min(self.height().max(0) as f32);
        self.imp().top.set(top);
        if let Some(surface) = self.surface() {
            let (_, translate_y) = self.surface_transform();
            let surface_top = (f64::from(top) - translate_y).floor().max(0.0) as i32;
            let region = gtk::cairo::Region::create_rectangle(&gtk::cairo::RectangleInt::new(
                0,
                surface_top,
                surface.width(),
                (surface.height() - surface_top).max(0),
            ));
            surface.set_input_region(Some(&region));
            surface.set_opaque_region(None);
        }
    }
}
pub(super) fn set_visible_top(window: &gtk::Window, marker: Option<&(gtk::Widget, gtk::Widget)>) {
    if let Some(window) = window.downcast_ref::<Window>() {
        *window.imp().marker.borrow_mut() =
            marker.map(|(widget, decoration)| (widget.downgrade(), decoration.downgrade()));
        window.update_region();
        // Redraw to apply the updated paint clip alongside the input region.
        window.queue_draw();
    }
}
