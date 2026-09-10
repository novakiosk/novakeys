use crate::config::AppConfig;
use crate::constants::geometry::*;
use anyhow::Result;
use relm4::gtk;
use relm4::gtk::prelude::*;

pub fn setup_css_providers(app_config: &AppConfig) -> Result<Vec<gtk::CssProvider>> {
    fn parse(css: &str) -> Result<gtk::CssProvider> {
        let provider = gtk::CssProvider::new();
        let errors = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let capture = errors.clone();
        let handler = provider
            .connect_parsing_error(move |_, _, error| capture.borrow_mut().push(error.to_string()));
        provider.load_from_data(css);
        provider.disconnect(handler);
        anyhow::ensure!(
            errors.borrow().is_empty(),
            "Invalid CSS: {}",
            errors.borrow().join("; ")
        );
        Ok(provider)
    }
    let default_provider = parse(super::style::DEFAULT_CSS)?;
    let user_provider = app_config
        .get_user_css_override()
        .map(|css| parse(&css))
        .transpose()?;
    let mut providers = vec![default_provider];
    providers.extend(user_provider);
    // Window painting happens outside its snapshot override. Keep the root clear;
    // user themes still style the clipped backdrop, keyboard and controls.
    providers.push(parse(
        "window.novakeys-overlay { background: transparent; border: none; box-shadow: none; }",
    )?);
    Ok(providers)
}

pub fn install_css_providers(providers: &[gtk::CssProvider]) {
    if let Some(display) = gtk::gdk::Display::default() {
        let default_provider = &providers[0];
        let invariant_provider = providers.last().unwrap();
        let user_provider = (providers.len() == 3).then(|| &providers[1]);
        gtk::style_context_add_provider_for_display(
            &display,
            default_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        if let Some(up) = user_provider {
            gtk::style_context_add_provider_for_display(
                &display,
                up,
                gtk::STYLE_PROVIDER_PRIORITY_USER,
            );
        }
        gtk::style_context_add_provider_for_display(
            &display,
            invariant_provider,
            gtk::STYLE_PROVIDER_PRIORITY_USER + 1,
        );
    }
}

pub fn configure_window_layout(root: &gtk::Window, app_config: &AppConfig) -> (i32, i32, bool) {
    use gtk4_layer_shell::{Edge, KeyboardMode, Layer, LayerShell};

    if !root.is_layer_window() {
        root.init_layer_shell();
    }
    root.set_layer(Layer::Overlay);
    root.set_keyboard_mode(KeyboardMode::None);

    let is_stretch = app_config.stretch();
    root.set_anchor(Edge::Bottom, true);
    root.set_anchor(Edge::Left, is_stretch);
    root.set_anchor(Edge::Right, is_stretch);
    let window_height = DEFAULT_WINDOW_HEIGHT;
    let kb_width = app_config.keyboard_width();
    root.set_default_size(if is_stretch { -1 } else { kb_width }, window_height);
    root.set_size_request(if is_stretch { -1 } else { kb_width }, window_height);
    root.set_resizable(false);

    (window_height, kb_width, is_stretch)
}

pub fn create_container_hierarchy(root: &gtk::Window, kb_width: i32, is_stretch: bool) -> gtk::Box {
    let backdrop = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();
    backdrop.add_css_class("novakeys-backdrop");

    if is_stretch {
        backdrop.set_hexpand(true);
        backdrop.set_vexpand(false);
        backdrop.set_halign(gtk::Align::Fill);
        backdrop.set_valign(gtk::Align::End);
        backdrop.set_width_request(-1);
    } else {
        backdrop.set_hexpand(false);
        backdrop.set_vexpand(false);
        backdrop.set_halign(gtk::Align::Center);
        backdrop.set_valign(gtk::Align::End);
        backdrop.set_width_request(kb_width);
    }

    if is_stretch {
        backdrop.add_css_class("stretch");
    }

    let container = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();
    container.add_css_class("novakeys-keyboard");

    if is_stretch {
        container.set_width_request(kb_width);
        container.set_hexpand(false);
        container.set_halign(gtk::Align::Center);
    } else {
        container.set_hexpand(true);
        container.set_halign(gtk::Align::Fill);
    }

    backdrop.append(&container);
    root.set_child(Some(&backdrop));

    container
}
