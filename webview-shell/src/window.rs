//! A tao window holding a wry webview. Blocks until the window is closed.

use anyhow::Result;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

pub fn open(url: &str, title: &str, size: (f64, f64)) -> Result<()> {
    let event_loop = EventLoop::new();
    #[cfg(target_os = "macos")]
    let menu = menu_bar(title)?;
    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(LogicalSize::new(size.0, size.1))
        .build(&event_loop)
        .map_err(|e| anyhow::anyhow!("creating window: {e}"))?;
    let _webview = WebViewBuilder::new()
        .with_url(url)
        .build(&window)
        .map_err(|e| anyhow::anyhow!("creating webview: {e}"))?;
    event_loop.run(move |event, _, control_flow| {
        // The menu bar lives as long as the window does.
        #[cfg(target_os = "macos")]
        let _ = &menu;
        *control_flow = ControlFlow::Wait;
        if let Event::WindowEvent {
            event: WindowEvent::CloseRequested,
            ..
        } = event
        {
            *control_flow = ControlFlow::Exit;
        }
    });
}

/// macOS sends Cmd-C, Cmd-V and the rest through the menu bar, and a tao
/// window has none, so without this the webview never sees them: text can
/// be selected but not copied.
#[cfg(target_os = "macos")]
fn menu_bar(title: &str) -> Result<muda::Menu> {
    use muda::{Menu, PredefinedMenuItem as Item, Submenu};
    let app = Submenu::with_items(
        title,
        true,
        &[
            &Item::hide(None),
            &Item::hide_others(None),
            &Item::show_all(None),
            &Item::separator(),
            &Item::quit(None),
        ],
    )?;
    let edit = Submenu::with_items(
        "Edit",
        true,
        &[
            &Item::undo(None),
            &Item::redo(None),
            &Item::separator(),
            &Item::cut(None),
            &Item::copy(None),
            &Item::paste(None),
            &Item::select_all(None),
        ],
    )?;
    let window = Submenu::with_items(
        "Window",
        true,
        &[&Item::minimize(None), &Item::close_window(None)],
    )?;
    let menu = Menu::with_items(&[&app, &edit, &window])?;
    menu.init_for_nsapp();
    Ok(menu)
}
