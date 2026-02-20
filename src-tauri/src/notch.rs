use crate::models::NotchInfo;

#[cfg(target_os = "macos")]
pub fn detect_notch() -> NotchInfo {
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

    unsafe {
        let ns_screen_class = match Class::get("NSScreen") {
            Some(c) => c,
            None => return NotchInfo::no_notch(1512.0, 982.0),
        };

        let main_screen: *mut Object = msg_send![ns_screen_class, mainScreen];
        if main_screen.is_null() {
            return NotchInfo::no_notch(1512.0, 982.0);
        }

        let frame: CGRect = msg_send![main_screen, frame];
        let screen_width = frame.size.width;
        let screen_height = frame.size.height;

        // Check if the screen responds to auxiliaryTopLeftArea (macOS 12+)
        let sel_aux = sel!(auxiliaryTopLeftArea);
        let responds: BOOL = msg_send![main_screen, respondsToSelector: sel_aux];

        if responds != YES {
            return NotchInfo::no_notch(screen_width, screen_height);
        }

        let left_area: CGRect = msg_send![main_screen, auxiliaryTopLeftArea];
        let right_area: CGRect = msg_send![main_screen, auxiliaryTopRightArea];

        if left_area.size.width > 0.0 && right_area.size.width > 0.0 {
            let notch_x = left_area.origin.x + left_area.size.width;
            let notch_width = right_area.origin.x - notch_x;
            let notch_height = left_area.size.height;

            NotchInfo {
                exists: true,
                x: notch_x,
                y: 0.0,
                width: notch_width,
                height: notch_height,
                screen_width,
                screen_height,
            }
        } else {
            NotchInfo::no_notch(screen_width, screen_height)
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub fn detect_notch() -> NotchInfo {
    NotchInfo::no_notch(1512.0, 982.0)
}
