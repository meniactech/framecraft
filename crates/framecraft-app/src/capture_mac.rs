// SPDX-License-Identifier: AGPL-3.0-or-later
//! Native frame capture (macOS): the graphic's preview page loaded in a
//! hidden WKWebView of the scene's size (in an invisible borderless
//! window, as WebKit paints only views in a window), seeked with
//! `goToTime` and captured with WebKit's `takeSnapshot`; scaled to the
//! scene's size (Retina snapshots are 2×). The same API as the Linux
//! capture (`capture.rs`).

use block2::RcBlock;
use dioxus::desktop::wry::WebViewExtMacOS;
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, msg_send};
use objc2_app_kit::{
    NSBackingStoreType, NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSWindow,
    NSWindowStyleMask,
};
use objc2_foundation::{
    NSDictionary, NSError, NSNumber, NSPoint, NSRect, NSSize, NSString, NSURL, NSURLRequest,
    ns_string,
};
use objc2_web_kit::{WKSnapshotConfiguration, WKWebView};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

pub struct Capture {
    window: Retained<NSWindow>,
    view: Retained<WKWebView>,
    width: u32,
    height: u32,
}

/// Waits (polling the main loop's timer) until `slot` is filled.
async fn wait<T>(slot: &Rc<RefCell<Option<T>>>, timeout: Duration) -> Option<T> {
    let step = Duration::from_millis(5);
    let mut waited = Duration::ZERO;
    loop {
        if let Some(v) = slot.borrow_mut().take() {
            return Some(v);
        }
        if waited >= timeout {
            return None;
        }
        tokio::time::sleep(step).await;
        waited += step;
    }
}

impl Capture {
    /// Loads `url` (a preview page) at `width` × `height` and waits until
    /// its runtime can seek.
    pub async fn open(url: &str, width: f64, height: f64) -> Result<Self, String> {
        let mtm = MainThreadMarker::new().ok_or("Capture must run on the main thread")?;
        let main = dioxus::desktop::window().webview.webview();
        // The app's configuration, so the framecraft:// handler is there.
        let config = unsafe { main.configuration() };
        let (w, h) = (width.round().max(1.0), height.round().max(1.0));
        let frame = NSRect::new(NSPoint::new(0.0, 0.0), NSSize::new(w, h));
        let window = unsafe {
            NSWindow::initWithContentRect_styleMask_backing_defer(
                NSWindow::alloc(mtm),
                NSRect::new(NSPoint::new(-30000.0, -30000.0), NSSize::new(w, h)),
                NSWindowStyleMask::Borderless,
                NSBackingStoreType::Buffered,
                false,
            )
        };
        unsafe { window.setReleasedWhenClosed(false) };
        window.setAlphaValue(0.0);
        window.setIgnoresMouseEvents(true);
        let view = unsafe {
            WKWebView::initWithFrame_configuration(WKWebView::alloc(mtm), frame, &config)
        };
        // Transparent page background (WebKit's drawsBackground switch).
        let no = NSNumber::numberWithBool(false);
        let _: () =
            unsafe { msg_send![&*view, setValue: &*no, forKey: ns_string!("drawsBackground")] };
        window.setContentView(Some(&view));
        window.orderFrontRegardless();
        let nsurl = NSURL::URLWithString(&NSString::from_str(url)).ok_or("Bad preview URL")?;
        unsafe { view.loadRequest(&NSURLRequest::requestWithURL(&nsurl)) };
        let capture = Self {
            window,
            view,
            width: w as u32,
            height: h as u32,
        };
        let mut ready = false;
        for _ in 0..300 {
            tokio::time::sleep(Duration::from_millis(50)).await;
            if !unsafe { capture.view.isLoading() }
                && capture
                    .eval("String(typeof window.goToTime === 'function')")
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
        tokio::time::sleep(Duration::from_millis(500)).await;
        Ok(capture)
    }

    /// Runs `script`; its result as text (strings only).
    async fn eval(&self, script: &str) -> Result<String, String> {
        let slot: Rc<RefCell<Option<Result<String, String>>>> = Rc::default();
        let s = slot.clone();
        let done = RcBlock::new(move |value: *mut AnyObject, error: *mut NSError| {
            let r = if let Some(e) = unsafe { error.as_ref() } {
                Err(e.localizedDescription().to_string())
            } else {
                Ok(unsafe { value.as_ref() }
                    .and_then(|v| v.downcast_ref::<NSString>())
                    .map(|s| s.to_string())
                    .unwrap_or_default())
            };
            *s.borrow_mut() = Some(r);
        });
        unsafe {
            self.view
                .evaluateJavaScript_completionHandler(&NSString::from_str(script), Some(&done));
        }
        wait(&slot, Duration::from_secs(10))
            .await
            .unwrap_or_else(|| Err("Script timed out".into()))
    }

    /// The graphic at `time_ms`, as a PNG of the scene's size.
    pub async fn frame(&self, time_ms: f64) -> Result<Vec<u8>, String> {
        self.eval(&format!(
            "window.goToTime({{ timestamp: {time_ms} }}); 'ok'"
        ))
        .await?;
        let mtm = MainThreadMarker::new().ok_or("Capture must run on the main thread")?;
        let cfg = unsafe { WKSnapshotConfiguration::new(mtm) };
        unsafe { cfg.setAfterScreenUpdates(true) };
        let slot: Rc<RefCell<Option<Result<Vec<u8>, String>>>> = Rc::default();
        let s = slot.clone();
        let done = RcBlock::new(move |image: *mut NSImage, error: *mut NSError| {
            let r = match (unsafe { image.as_ref() }, unsafe { error.as_ref() }) {
                (_, Some(e)) => Err(e.localizedDescription().to_string()),
                (Some(img), None) => img
                    .TIFFRepresentation()
                    .and_then(|tiff| NSBitmapImageRep::imageRepWithData(&tiff))
                    .and_then(|rep| unsafe {
                        rep.representationUsingType_properties(
                            NSBitmapImageFileType::PNG,
                            &NSDictionary::new(),
                        )
                    })
                    .map(|png| png.to_vec())
                    .ok_or_else(|| "Could not encode the snapshot".to_string()),
                (None, None) => Err("Empty snapshot".into()),
            };
            *s.borrow_mut() = Some(r);
        });
        unsafe {
            self.view
                .takeSnapshotWithConfiguration_completionHandler(Some(&cfg), &done);
        }
        let png = wait(&slot, Duration::from_secs(10))
            .await
            .unwrap_or_else(|| Err("Snapshot timed out".into()))?;
        crate::pngfit::fit_png(png, self.width, self.height)
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.window.setContentView(None);
        self.window.close();
        let _ = &self.view;
    }
}
