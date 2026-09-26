//! A tao window holding a wry webview. Blocks until the window is closed.

use anyhow::Result;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::{NewWindowResponse, WebViewBuilder};

/// `icon` is a PNG for the Dock, on macOS; elsewhere it is not used.
pub fn open(url: &str, title: &str, size: (f64, f64), icon: Option<&[u8]>) -> Result<()> {
    let event_loop = EventLoop::new();
    #[cfg(target_os = "macos")]
    let menu = menu_bar(title)?;
    let window = WindowBuilder::new()
        .with_title(title)
        .with_inner_size(LogicalSize::new(size.0, size.1))
        .build(&event_loop)
        .map_err(|e| anyhow::anyhow!("creating window: {e}"))?;
    // The app's own pages stay in the window; a link anywhere else opens in
    // the browser, since a webview is a poor place to read mail.
    let origin = url.trim_end_matches('/').to_string();
    let _webview = WebViewBuilder::new()
        .with_url(url)
        .with_navigation_handler(move |target: String| {
            if target.starts_with(&origin) || target.starts_with("about:") {
                return true;
            }
            let _ = opener::open(&target);
            false
        })
        .with_new_window_req_handler(|target: String, _| {
            let _ = opener::open(&target);
            NewWindowResponse::Deny
        })
        .build(&window)
        .map_err(|e| anyhow::anyhow!("creating webview: {e}"))?;
    #[cfg(target_os = "macos")]
    if let Some(png) = icon {
        dock_icon(png);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = icon;
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

/// Show `png` as the app's icon in the Dock, shaped like the other icons
/// there: a rounded square with a margin, on Apple's 1024-point grid.
/// A binary started from a terminal has no bundle to carry an icon, so
/// without this the Dock shows a generic one.
#[cfg(target_os = "macos")]
fn dock_icon(png: &[u8]) {
    use objc2::AllocAnyThread;
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSBezierPath, NSCompositingOperation, NSImage};
    use objc2_foundation::{NSData, NSPoint, NSRect, NSSize};

    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let data = NSData::with_bytes(png);
    let Some(source) = NSImage::initWithData(NSImage::alloc(), &data) else {
        return;
    };
    const SIDE: f64 = 1024.0;
    const SQUARE: f64 = 824.0;
    const RADIUS: f64 = 185.0;
    let inset = (SIDE - SQUARE) / 2.0;
    let square = NSRect::new(NSPoint::new(inset, inset), NSSize::new(SQUARE, SQUARE));
    let canvas = NSImage::initWithSize(NSImage::alloc(), NSSize::new(SIDE, SIDE));
    #[allow(deprecated)]
    canvas.lockFocus();
    NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(square, RADIUS, RADIUS).addClip();
    source.drawInRect_fromRect_operation_fraction(
        square,
        NSRect::ZERO,
        NSCompositingOperation::SourceOver,
        1.0,
    );
    #[allow(deprecated)]
    canvas.unlockFocus();
    unsafe { NSApplication::sharedApplication(mtm).setApplicationIconImage(Some(&canvas)) };
}
