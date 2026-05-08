use super::*;

#[repr(C)]
#[derive(Clone, Copy)]
pub(super) struct ArcadeLrps2MetalHostV1 {
    pub version: u32,
    pub ns_view: *mut c_void,
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub presented_callback: Option<extern "C" fn(*mut c_void)>,
    pub callback_user_data: *mut c_void,
}

pub(super) type ArcadeLrps2SetMetalHostV1Fn =
    unsafe extern "C" fn(*const ArcadeLrps2MetalHostV1) -> bool;

pub(super) struct ExternalMacosMetalWindow {
    ns_window: *mut Object,
    content_view: *mut Object,
    width: u32,
    height: u32,
    scale: f32,
    visible: bool,
}

unsafe impl Send for ExternalMacosMetalWindow {}

unsafe fn set_ns_window_title(ns_window: *mut Object, title: &str) {
    let sanitized = title.replace('\0', " ");
    let Ok(title) = CString::new(sanitized) else {
        return;
    };
    let ns_string_class = class!(NSString);
    let ns_title: *mut Object = msg_send![ns_string_class, stringWithUTF8String: title.as_ptr()];
    let _: () = msg_send![ns_window, setTitle: ns_title];
}

impl ExternalMacosMetalWindow {
    pub(super) fn create(title: &str) -> std::result::Result<Self, String> {
        unsafe {
            let ns_app_class = class!(NSApplication);
            let _: *mut Object = msg_send![ns_app_class, sharedApplication];

            let display_id = CGMainDisplayID();
            let width = (CGDisplayPixelsWide(display_id) as u32).max(640);
            let height = (CGDisplayPixelsHigh(display_id) as u32).max(480);

            let content_rect = NSRect {
                origin: NSPoint { x: 0.0, y: 0.0 },
                size: NSSize {
                    width: width as f64,
                    height: height as f64,
                },
            };
            let style_mask: usize = 0;
            let backing: usize = 2;

            let ns_window_class = class!(NSWindow);
            let ns_window: *mut Object = msg_send![ns_window_class, alloc];
            let ns_window: *mut Object = msg_send![
                ns_window,
                initWithContentRect:content_rect
                styleMask:style_mask
                backing:backing
                defer:NO
            ];
            if ns_window.is_null() {
                return Err(String::from(
                    "failed to create NSWindow for Metal presentation",
                ));
            }

            let content_view: *mut Object = msg_send![ns_window, contentView];
            if content_view.is_null() {
                let _: () = msg_send![ns_window, release];
                return Err(String::from(
                    "failed to get NSWindow content view for Metal presentation",
                ));
            }
            let _: () = msg_send![content_view, retain];

            set_ns_window_title(ns_window, title);
            let ns_color_class = class!(NSColor);
            let black: *mut Object = msg_send![ns_color_class, blackColor];
            let _: () = msg_send![ns_window, setBackgroundColor: black];

            Ok(Self {
                ns_window,
                content_view,
                width,
                height,
                scale: 1.0,
                visible: false,
            })
        }
    }

    pub(super) fn host_payload(&self, callback_user_data: *mut c_void) -> ArcadeLrps2MetalHostV1 {
        ArcadeLrps2MetalHostV1 {
            version: 1,
            ns_view: self.content_view.cast(),
            width: self.width,
            height: self.height,
            scale: self.scale,
            presented_callback: Some(arcade_lrps2_metal_presented),
            callback_user_data,
        }
    }

    pub(super) fn set_title(&mut self, title: &str) {
        unsafe {
            set_ns_window_title(self.ns_window, title);
        }
    }

    pub(super) fn set_visible(&mut self, visible: bool) {
        if visible == self.visible {
            return;
        }
        unsafe {
            let null: *mut Object = std::ptr::null_mut();
            if visible {
                let fullscreen_rect = NSRect {
                    origin: NSPoint { x: 0.0, y: 0.0 },
                    size: NSSize {
                        width: self.width as f64,
                        height: self.height as f64,
                    },
                };
                let _: () = msg_send![self.ns_window, setFrame:fullscreen_rect display:YES];
                let _: () = msg_send![self.ns_window, makeKeyAndOrderFront: null];
            } else {
                let _: () = msg_send![self.ns_window, orderOut: null];
            }
        }
        self.visible = visible;
    }
}

pub(super) fn destroy_external_macos_metal_window(window: ExternalMacosMetalWindow) {
    unsafe {
        let _: () = msg_send![window.content_view, setLayer: std::ptr::null_mut::<Object>()];
        let _: () = msg_send![window.content_view, setWantsLayer: NO];
        let _: () = msg_send![window.content_view, release];
        let _: () = msg_send![window.ns_window, close];
    }
}

extern "C" fn arcade_lrps2_metal_presented(user_data: *mut c_void) {
    if user_data.is_null() {
        return;
    }
    let runtime = unsafe { &*(user_data as *const HostRuntime) };
    {
        let mut state = runtime.hw_render_state.lock();
        state.macos_metal_present_active = true;
        state.macos_metal_visibility_pending = Some(true);
    }
    let mut metrics = runtime.vulkan_present_metrics.lock();
    metrics.queue_present_successes = metrics.queue_present_successes.saturating_add(1);
    metrics.external_present_deliveries = metrics.external_present_deliveries.saturating_add(1);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_payload_exposes_content_ns_view() {
        let content_view = 0x20usize as *mut Object;
        let window = ExternalMacosMetalWindow {
            ns_window: 0x10usize as *mut Object,
            content_view,
            width: 640,
            height: 480,
            scale: 2.0,
            visible: false,
        };

        let payload = window.host_payload(std::ptr::null_mut());

        assert_eq!(payload.version, 1);
        assert_eq!(payload.ns_view, content_view.cast());
        assert_eq!(payload.width, 640);
        assert_eq!(payload.height, 480);
        assert_eq!(payload.scale, 2.0);
        assert!(payload.presented_callback.is_some());
    }

    #[test]
    fn first_present_callback_marks_metal_presentation_ready() {
        let runtime = HostRuntime::default();
        let user_data = (&runtime as *const HostRuntime).cast_mut().cast();

        arcade_lrps2_metal_presented(user_data);

        let state = runtime.hw_render_state.lock();
        assert!(state.macos_metal_present_active);
        assert_eq!(state.macos_metal_visibility_pending, Some(true));
        drop(state);
        assert_eq!(
            runtime
                .vulkan_present_metrics
                .lock()
                .external_present_deliveries,
            1
        );
    }
}
