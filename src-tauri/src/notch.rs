use crate::models::NotchInfo;

/// Result of notch detection, containing both the frontend-facing info
/// and internal values needed by the hover monitor.
pub struct NotchDetection {
    pub info: NotchInfo,
    /// Top of the notch screen in macOS global coordinates (bottom-left origin).
    /// Used by the hover monitor to convert mouse y to distance-from-top.
    pub screen_top_macos_y: f64,
}

#[cfg(target_os = "macos")]
pub fn detect_notch() -> NotchDetection {
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{msg_send, sel, sel_impl};

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGPoint {
        x: f64,
        y: f64,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGSize {
        width: f64,
        height: f64,
    }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGRect {
        origin: CGPoint,
        size: CGSize,
    }

    let fallback = || {
        let info = NotchInfo::no_notch(1512.0, 982.0);
        let screen_top = info.screen_height;
        NotchDetection {
            info,
            screen_top_macos_y: screen_top,
        }
    };

    unsafe {
        let ns_screen_class = match Class::get("NSScreen") {
            Some(c) => c,
            None => return fallback(),
        };

        // Get all screens. The first screen is always the primary display
        // (the one at origin (0,0) in macOS global coordinates).
        let screens: *mut Object = msg_send![ns_screen_class, screens];
        let count: usize = msg_send![screens, count];
        if count == 0 {
            return fallback();
        }

        // Use screens[0] (the primary display) for Tauri y-coordinate conversion.
        // Note: NSScreen.mainScreen returns the screen with the key window, NOT
        // the primary display — using it here would break coordinate math when
        // a non-primary monitor has focus.
        let primary_screen: *mut Object = msg_send![screens, objectAtIndex: 0usize];
        let primary_frame: CGRect = msg_send![primary_screen, frame];
        let primary_height = primary_frame.size.height;

        let sel_aux = sel!(auxiliaryTopLeftArea);

        for i in 0..count {
            let screen: *mut Object = msg_send![screens, objectAtIndex: i];
            if screen.is_null() {
                continue;
            }

            let responds: BOOL = msg_send![screen, respondsToSelector: sel_aux];
            if responds != YES {
                continue;
            }

            let left_area: CGRect = msg_send![screen, auxiliaryTopLeftArea];
            let right_area: CGRect = msg_send![screen, auxiliaryTopRightArea];

            if left_area.size.width > 0.0 && right_area.size.width > 0.0 {
                let frame: CGRect = msg_send![screen, frame];
                let screen_width = frame.size.width;
                let screen_height = frame.size.height;
                let screen_top_macos_y = frame.origin.y + screen_height;

                let notch_x = left_area.origin.x + left_area.size.width;
                let notch_width = right_area.origin.x - notch_x;
                let notch_height = left_area.size.height;

                // Convert screen top to Tauri coordinates (top-left origin,
                // where y=0 is the top of the primary screen).
                let tauri_y = primary_height - screen_top_macos_y;

                return NotchDetection {
                    info: NotchInfo {
                        exists: true,
                        x: notch_x,
                        y: tauri_y,
                        width: notch_width,
                        height: notch_height,
                        screen_width,
                        screen_height,
                    },
                    screen_top_macos_y,
                };
            }
        }

        // No notch found on any screen — fall back to primary screen center.
        let screen_width = primary_frame.size.width;
        let screen_height = primary_frame.size.height;
        NotchDetection {
            info: NotchInfo::no_notch(screen_width, screen_height),
            screen_top_macos_y: primary_height,
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn detect_notch() -> NotchDetection {
    let info = NotchInfo::no_notch(1512.0, 982.0);
    NotchDetection {
        info,
        screen_top_macos_y: 982.0,
    }
}
