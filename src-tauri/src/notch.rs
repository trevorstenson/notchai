use crate::models::{NotchInfo, ScreenInfo};

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

/// Enumerate all connected screens with metadata.
#[cfg(target_os = "macos")]
pub fn list_screens() -> Vec<ScreenInfo> {
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{msg_send, sel, sel_impl};

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGPoint { x: f64, y: f64 }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGSize { width: f64, height: f64 }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGRect { origin: CGPoint, size: CGSize }

    let mut result = Vec::new();

    unsafe {
        let ns_screen_class = match Class::get("NSScreen") {
            Some(c) => c,
            None => return result,
        };

        let screens: *mut Object = msg_send![ns_screen_class, screens];
        let count: usize = msg_send![screens, count];

        let sel_aux = sel!(auxiliaryTopLeftArea);
        let sel_name = sel!(localizedName);

        for i in 0..count {
            let screen: *mut Object = msg_send![screens, objectAtIndex: i];
            if screen.is_null() {
                continue;
            }

            let frame: CGRect = msg_send![screen, frame];
            let width = frame.size.width;
            let height = frame.size.height;

            // Get display name (macOS 10.15+)
            let name: String = {
                let responds_name: BOOL = msg_send![screen, respondsToSelector: sel_name];
                if responds_name == YES {
                    let ns_name: *mut Object = msg_send![screen, localizedName];
                    if !ns_name.is_null() {
                        let cstr: *const std::os::raw::c_char = msg_send![ns_name, UTF8String];
                        if !cstr.is_null() {
                            std::ffi::CStr::from_ptr(cstr)
                                .to_string_lossy()
                                .into_owned()
                        } else {
                            format!("Display {}", i + 1)
                        }
                    } else {
                        format!("Display {}", i + 1)
                    }
                } else {
                    format!("Display {}", i + 1)
                }
            };

            // Check for notch
            let has_notch = {
                let responds: BOOL = msg_send![screen, respondsToSelector: sel_aux];
                if responds == YES {
                    let left_area: CGRect = msg_send![screen, auxiliaryTopLeftArea];
                    let right_area: CGRect = msg_send![screen, auxiliaryTopRightArea];
                    left_area.size.width > 0.0 && right_area.size.width > 0.0
                } else {
                    false
                }
            };

            result.push(ScreenInfo {
                index: i,
                name,
                has_notch,
                width,
                height,
                is_primary: i == 0,
            });
        }
    }

    result
}

#[cfg(not(target_os = "macos"))]
pub fn list_screens() -> Vec<ScreenInfo> {
    vec![ScreenInfo {
        index: 0,
        name: "Display 1".to_string(),
        has_notch: false,
        width: 1512.0,
        height: 982.0,
        is_primary: true,
    }]
}

/// Detect notch on a specific screen, or auto-detect if `screen_index` is None.
#[cfg(target_os = "macos")]
pub fn detect_notch_on_screen(screen_index: Option<usize>) -> NotchDetection {
    use objc::runtime::{Class, Object, BOOL, YES};
    use objc::{msg_send, sel, sel_impl};

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGPoint { x: f64, y: f64 }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGSize { width: f64, height: f64 }

    #[repr(C)]
    #[derive(Debug, Clone, Copy)]
    struct CGRect { origin: CGPoint, size: CGSize }

    let idx = match screen_index {
        None => return detect_notch(),
        Some(i) => i,
    };

    let fallback = || {
        let info = NotchInfo::no_notch(1512.0, 982.0);
        let screen_top = info.screen_height;
        NotchDetection { info, screen_top_macos_y: screen_top }
    };

    unsafe {
        let ns_screen_class = match Class::get("NSScreen") {
            Some(c) => c,
            None => return fallback(),
        };

        let screens: *mut Object = msg_send![ns_screen_class, screens];
        let count: usize = msg_send![screens, count];

        // If the saved display index is stale (monitor removed/reordered),
        // fall back to normal auto-detection instead of hardcoded coordinates.
        if idx >= count {
            return detect_notch();
        }

        let primary_screen: *mut Object = msg_send![screens, objectAtIndex: 0usize];
        let primary_frame: CGRect = msg_send![primary_screen, frame];
        let primary_height = primary_frame.size.height;

        let screen: *mut Object = msg_send![screens, objectAtIndex: idx];
        if screen.is_null() {
            return detect_notch();
        }

        let frame: CGRect = msg_send![screen, frame];
        let screen_width = frame.size.width;
        let screen_height = frame.size.height;
        let screen_top_macos_y = frame.origin.y + screen_height;
        let tauri_y = primary_height - screen_top_macos_y;

        // Check if this screen has a notch
        let sel_aux = sel!(auxiliaryTopLeftArea);
        let responds: BOOL = msg_send![screen, respondsToSelector: sel_aux];
        if responds == YES {
            let left_area: CGRect = msg_send![screen, auxiliaryTopLeftArea];
            let right_area: CGRect = msg_send![screen, auxiliaryTopRightArea];

            if left_area.size.width > 0.0 && right_area.size.width > 0.0 {
                let notch_x = left_area.origin.x + left_area.size.width;
                let notch_width = right_area.origin.x - notch_x;
                let notch_height = left_area.size.height;

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

        // No notch on this screen — center-top position
        let width = 200.0;
        NotchDetection {
            info: NotchInfo {
                exists: false,
                x: frame.origin.x + (screen_width - width) / 2.0,
                y: tauri_y,
                width,
                height: 32.0,
                screen_width,
                screen_height,
            },
            screen_top_macos_y,
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn detect_notch_on_screen(screen_index: Option<usize>) -> NotchDetection {
    let _ = screen_index;
    detect_notch()
}
