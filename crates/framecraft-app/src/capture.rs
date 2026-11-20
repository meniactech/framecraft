// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native frame capture (Linux): the graphic's preview page loaded in a
//! hidden WebKit view of the scene's exact size, seeked with `goToTime`
//! and captured by WebKit itself — everything the viewport shows
//! (canvases such as particles and charts, images, video, fonts), with
//! transparency. In-page capture (html-to-image) paints no images on
//! WebKitGTK. Other systems use the in-page capture for now.

use dioxus::desktop::wry::WebViewExtUnix;
use gtk::prelude::*;
use javascriptcore::ValueExt;
use std::time::Duration;
use webkit2gtk::{SnapshotOptions, SnapshotRegion, WebViewExt};

pub struct Capture {
    window: gtk::OffscreenWindow,
    view: webkit2gtk::WebView,
    width: i32,
    height: i32,
}

impl Capture {
    /// Loads `url` (a preview page) at `width` × `height` and waits until
    /// its runtime can seek.
    pub async fn open(url: &str, width: f64, height: f64) -> Result<Self, String> {
        let main = dioxus::desktop::window().webview.webview();
        // The same web context as the app, so framecraft:// pages load.
        let context = main.context().ok_or("No web context")?;
        let view = webkit2gtk::WebView::with_context(&context);
        view.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
        let (w, h) = (
            width.round().max(1.0) as i32,
            height.round().max(1.0) as i32,
        );
        view.set_size_request(w, h);
        let window = gtk::OffscreenWindow::new();
        window.set_default_size(w, h);
        window.add(&view);
        window.show_all();
        view.load_uri(url);
        let capture = Self {
            window,
            view,
            width: w,
            height: h,
        };
        // Loaded, and the runtime's goToTime is there (at most 15 s).
        let mut ready = false;
        for _ in 0..300 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if !capture.view.is_loading()
                && capture
                    .eval("typeof window.goToTime === 'function'")
                    .await
                    .is_ok_and(|v| v == "true")
            {
                ready = true;
                break;
            }
        }
        if !ready {
            return Err("The graphic did not load for capture".into());
        }
        // Fonts and images a moment to arrive.
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(capture)
    }

    async fn eval(&self, script: &str) -> Result<String, String> {
        self.view
            .evaluate_javascript_future(script, None, None)
            .await
            .map(|v| v.to_str().to_string())
            .map_err(|e| e.to_string())
    }

    /// The graphic at `time_ms`, as a PNG of the scene's size.
    pub async fn frame(&self, time_ms: f64) -> Result<Vec<u8>, String> {
        self.eval(&format!(
            "window.goToTime({{ timestamp: {time_ms} }}); true"
        ))
        .await?;
        let shot = self
            .view
            .snapshot_future(
                SnapshotRegion::Visible,
                SnapshotOptions::TRANSPARENT_BACKGROUND,
            )
            .await
            .map_err(|e| e.to_string())?;
        // The snapshot is in device pixels (2× on HiDPI): scale it to the
        // scene's size.
        let (sw, sh) = match cairo::ImageSurface::try_from(shot.clone()) {
            Ok(img) => (f64::from(img.width()), f64::from(img.height())),
            Err(_) => (f64::from(self.width), f64::from(self.height)),
        };
        let out = cairo::ImageSurface::create(cairo::Format::ARgb32, self.width, self.height)
            .map_err(|e| e.to_string())?;
        {
            let cr = cairo::Context::new(&out).map_err(|e| e.to_string())?;
            cr.scale(
                f64::from(self.width) / sw.max(1.0),
                f64::from(self.height) / sh.max(1.0),
            );
            cr.set_source_surface(&shot, 0.0, 0.0)
                .map_err(|e| e.to_string())?;
            cr.paint().map_err(|e| e.to_string())?;
        }
        let mut png = Vec::new();
        out.write_to_png(&mut png).map_err(|e| e.to_string())?;
        Ok(png)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        // SAFETY: the window and its view are only used by this struct.
        unsafe { self.window.destroy() };
    }
}
