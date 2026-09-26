//! A tao window holding a wry webview. Blocks until the window is closed.

use anyhow::Result;
use tao::dpi::LogicalSize;
use tao::event::{Event, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoop};
use tao::window::WindowBuilder;
use wry::WebViewBuilder;

pub fn open(url: &str, title: &str, size: (f64, f64)) -> Result<()> {
    let event_loop = EventLoop::new();
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
