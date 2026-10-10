//! Web views laid over the shell, on Linux.
//!
//! On Windows and macOS a child web view is a native layer at the position it
//! is given. On Linux, Tauri packs every web view of a window into the same
//! vertical GtkBox, so a child was stacked *under* the shell instead of over
//! it, and position and size calls are no-ops there.
//!
//! Here the shell is moved into a GtkOverlay as its main child (it fills the
//! window) and every placed web view (the Store, the menu) becomes an overlay
//! child at exactly the rectangle it was given. Later ones stack on top, so
//! the menu, made after the Store, is always above it. Showing and hiding
//! still go through Tauri, since they act on the widget itself.
//!
//! Tauri's own edge resizing for a window without a frame finds the window
//! two parents up from the shell, which the overlay moved one level further
//! away: the window could no longer be resized. `resize_edges` does the same
//! job for the shell and the Store, measured against the window itself.

use std::cell::RefCell;
use std::collections::HashMap;

use gtk::prelude::*;

type Rect = (i32, i32, i32, i32);

thread_local! {
    // Main-thread only: every GTK call here runs inside `with_webview`.
    static RECTS: RefCell<HashMap<String, Rect>> = RefCell::new(HashMap::new());
    static OVERLAY: RefCell<Option<gtk::Overlay>> = const { RefCell::new(None) };
}

fn rect(x: f64, y: f64, width: f64, height: f64) -> Rect {
    (
        x.round() as i32,
        y.round() as i32,
        width.round().max(1.0) as i32,
        height.round().max(1.0) as i32,
    )
}

/// Put `view` over the shell (first call) or move it (later calls).
/// Coordinates are logical pixels in the main window, as the shell measures
/// them, which is also what GTK allocates in.
pub fn place(view: &tauri::Webview, x: f64, y: f64, width: f64, height: f64) {
    let r = rect(x, y, width, height);
    let name = view.label().to_string();
    let _ = view.with_webview(move |w| {
        let widget: gtk::Widget = w.inner().upcast();
        widget.set_widget_name(&name);
        RECTS.with(|m| m.borrow_mut().insert(name, r));
        let overlay = OVERLAY.with(|o| o.borrow().clone()).or_else(|| {
            let made = wrap_shell(&widget)?;
            OVERLAY.with(|o| *o.borrow_mut() = Some(made.clone()));
            Some(made)
        });
        let Some(overlay) = overlay else { return };
        if widget.parent().as_ref() != Some(overlay.upcast_ref()) {
            adopt(&overlay, &widget);
        }
        overlay.queue_resize();
    });
}

/// Move the shell out of Tauri's box into a new overlay that takes its place.
fn wrap_shell(placed: &gtk::Widget) -> Option<gtk::Overlay> {
    let vbox = placed.parent()?.downcast::<gtk::Box>().ok()?;
    // The shell was packed first; every other web view is placed here.
    let shell = vbox
        .children()
        .into_iter()
        .find(|c| c != placed && c.type_().name() == "WebKitWebView")?;
    let overlay = gtk::Overlay::new();
    // `shell` holds the widget alive across the remove.
    vbox.remove(&shell);
    overlay.add(&shell);
    overlay.connect_get_child_position(|_, child| {
        let name = child.widget_name();
        let (x, y, w, h) = RECTS.with(|m| m.borrow().get(name.as_str()).copied())?;
        Some(gtk::gdk::Rectangle::new(x, y, w, h))
    });
    vbox.pack_start(&overlay, true, true, 0);
    overlay.show();
    shell.show();
    resize_edges(&shell);
    Some(overlay)
}

/// Take a web view out of Tauri's box and lay it over the shell, keeping
/// whatever visibility it was last given.
fn adopt(overlay: &gtk::Overlay, widget: &gtk::Widget) {
    let shown = widget.is_visible();
    if let Some(parent) = widget.parent().and_then(|p| p.downcast::<gtk::Container>().ok()) {
        parent.remove(widget);
    }
    overlay.add_overlay(widget);
    if widget.widget_name() != crate::menus::LABEL {
        resize_edges(widget);
    }
    if shown {
        widget.show();
    } else {
        widget.hide();
    }
}

/// How close to the window's edge a press resizes it, in logical pixels
/// (Tauri's own inset).
const EDGE: i32 = 5;

/// The toplevel `widget` is in, when it is one we resize ourselves: no frame,
/// resizable, and not maximized.
fn resizable(widget: &gtk::Widget) -> Option<gtk::Window> {
    let window = widget.toplevel()?.downcast::<gtk::Window>().ok()?;
    (!window.is_decorated() && window.is_resizable() && !window.is_maximized()).then_some(window)
}

/// The edge of `window` under a point on the screen, if any.
fn edge_at(window: &gtk::Window, root_x: f64, root_y: f64) -> Option<gtk::gdk::WindowEdge> {
    use gtk::gdk::WindowEdge;
    let gdk = window.window()?;
    let (_, wx, wy) = gdk.origin();
    let (x, y) = (root_x - f64::from(wx), root_y - f64::from(wy));
    let (w, h) = (f64::from(gdk.width()), f64::from(gdk.height()));
    let border = f64::from(EDGE);
    let (left, right) = (x < border, x >= w - border);
    let (top, bottom) = (y < border, y >= h - border);
    Some(match (left, right, top, bottom) {
        (true, _, true, _) => WindowEdge::NorthWest,
        (_, true, true, _) => WindowEdge::NorthEast,
        (true, _, _, true) => WindowEdge::SouthWest,
        (_, true, _, true) => WindowEdge::SouthEast,
        (true, ..) => WindowEdge::West,
        (_, true, ..) => WindowEdge::East,
        (_, _, true, _) => WindowEdge::North,
        (_, _, _, true) => WindowEdge::South,
        _ => return None,
    })
}

fn edge_cursor(display: &gtk::gdk::Display, edge: gtk::gdk::WindowEdge) -> Option<gtk::gdk::Cursor> {
    use gtk::gdk::{CursorType, WindowEdge};
    let kind = match edge {
        WindowEdge::West => CursorType::LeftSide,
        WindowEdge::East => CursorType::RightSide,
        WindowEdge::North => CursorType::TopSide,
        WindowEdge::South => CursorType::BottomSide,
        WindowEdge::NorthWest => CursorType::TopLeftCorner,
        WindowEdge::NorthEast => CursorType::TopRightCorner,
        WindowEdge::SouthWest => CursorType::BottomLeftCorner,
        WindowEdge::SouthEast => CursorType::BottomRightCorner,
        _ => return None,
    };
    gtk::gdk::Cursor::for_display(display, kind)
}

/// Resize the window from the few pixels along its edge that `widget` covers.
fn resize_edges(widget: &gtk::Widget) {
    use gtk::glib::Propagation;
    widget.add_events(
        gtk::gdk::EventMask::POINTER_MOTION_MASK | gtk::gdk::EventMask::BUTTON_PRESS_MASK,
    );
    widget.connect_motion_notify_event(|widget, event| {
        let Some(window) = resizable(widget) else { return Propagation::Proceed };
        let (rx, ry) = event.root();
        let Some(edge) = edge_at(&window, rx, ry) else { return Propagation::Proceed };
        if let Some(gdk) = widget.window() {
            gdk.set_cursor(edge_cursor(&gdk.display(), edge).as_ref());
        }
        Propagation::Stop
    });
    widget.connect_button_press_event(|widget, event| {
        if event.button() != 1 {
            return Propagation::Proceed;
        }
        let Some(window) = resizable(widget) else { return Propagation::Proceed };
        let (rx, ry) = event.root();
        let Some(edge) = edge_at(&window, rx, ry) else { return Propagation::Proceed };
        window.begin_resize_drag(edge, 1, rx as i32, ry as i32, event.time());
        Propagation::Stop
    });
}
