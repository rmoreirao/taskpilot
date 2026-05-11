/// Single-instance enforcement using a Windows Named Mutex and Named Event.
///
/// - The mutex prevents multiple TaskPilot processes from running simultaneously.
/// - The event allows a second instance to signal the first to restore its window.
/// - The second instance also directly shows/foregrounds the first instance's
///   window via Win32 API, because eframe's `update()` loop (which polls the
///   event) does not run while the window is hidden.

#[cfg(windows)]
mod platform {
    use windows_sys::Win32::Foundation::{
        CloseHandle, GetLastError, HANDLE, WAIT_OBJECT_0, ERROR_ALREADY_EXISTS,
    };
    use windows_sys::Win32::System::Threading::{
        CreateEventW, CreateMutexW, OpenEventW, SetEvent, WaitForSingleObject,
        EVENT_MODIFY_STATE,
    };

    const MUTEX_NAME: &str = "Local\\TaskPilotSingleInstance";
    const EVENT_NAME: &str = "Local\\TaskPilotActivateEvent";

    /// Kept alive for the process lifetime to hold the mutex and event.
    pub struct SingleInstanceGuard {
        mutex_handle: HANDLE,
        event_handle: HANDLE,
    }

    // SAFETY: The Win32 handles are only accessed from the UI thread (poll in update()).
    unsafe impl Send for SingleInstanceGuard {}

    impl SingleInstanceGuard {
        /// Acquire the single-instance lock.
        ///
        /// If another instance already holds the mutex, this signals that instance
        /// to restore its window and then **exits the current process**.
        pub fn acquire() -> Self {
            let mutex_wide: Vec<u16> =
                MUTEX_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let mutex_handle =
                unsafe { CreateMutexW(std::ptr::null(), 0, mutex_wide.as_ptr()) };

            if mutex_handle.is_null() {
                eprintln!("Warning: failed to create single-instance mutex");
                return Self {
                    mutex_handle: std::ptr::null_mut(),
                    event_handle: std::ptr::null_mut(),
                };
            }

            if unsafe { GetLastError() } == ERROR_ALREADY_EXISTS {
                unsafe { CloseHandle(mutex_handle) };
                signal_existing_instance();
                std::process::exit(0);
            }

            // We are the first instance — create an auto-reset, non-signaled event
            // that later instances will signal to request activation.
            let event_wide: Vec<u16> =
                EVENT_NAME.encode_utf16().chain(std::iter::once(0)).collect();
            let event_handle =
                unsafe { CreateEventW(std::ptr::null(), 0, 0, event_wide.as_ptr()) };

            Self {
                mutex_handle,
                event_handle,
            }
        }

        /// Non-blocking check: returns `true` if another instance signaled us to
        /// bring our window to the foreground.  The event auto-resets after a
        /// successful wait, so each signal is consumed exactly once.
        pub fn check_activation(&self) -> bool {
            if self.event_handle.is_null() {
                return false;
            }
            unsafe { WaitForSingleObject(self.event_handle, 0) == WAIT_OBJECT_0 }
        }
    }

    impl Drop for SingleInstanceGuard {
        fn drop(&mut self) {
            if !self.event_handle.is_null() {
                unsafe { CloseHandle(self.event_handle) };
            }
            if !self.mutex_handle.is_null() {
                unsafe { CloseHandle(self.mutex_handle) };
            }
        }
    }

    /// Open the activation event owned by the first instance and signal it,
    /// then directly show and foreground the first instance's window via Win32.
    ///
    /// The second instance (us) is the foreground process because the user just
    /// launched it, so `SetForegroundWindow` will succeed — unlike calling it
    /// from the first instance which is a background process at this point.
    fn signal_existing_instance() {
        let event_wide: Vec<u16> =
            EVENT_NAME.encode_utf16().chain(std::iter::once(0)).collect();
        let handle = unsafe { OpenEventW(EVENT_MODIFY_STATE, 0, event_wide.as_ptr()) };
        if !handle.is_null() {
            unsafe {
                SetEvent(handle);
                CloseHandle(handle);
            }
        }

        // Directly show the first instance's window. We have foreground rights
        // because the user just launched us, and eframe's update() may not be
        // running while the window is hidden (no WM_PAINT → no RedrawRequested).
        restore_first_instance_window();
    }

    /// Find the first instance's window by title and bring it to the foreground.
    fn restore_first_instance_window() {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            FindWindowW, IsIconic, SetForegroundWindow, ShowWindow, SW_RESTORE, SW_SHOW,
        };

        let title: Vec<u16> = "TaskPilot\0".encode_utf16().collect();
        let hwnd = unsafe { FindWindowW(std::ptr::null(), title.as_ptr()) };
        if !hwnd.is_null() {
            unsafe {
                if IsIconic(hwnd) != 0 {
                    ShowWindow(hwnd, SW_RESTORE);
                } else {
                    ShowWindow(hwnd, SW_SHOW);
                }
                SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    /// No-op on non-Windows platforms.
    pub struct SingleInstanceGuard;
    impl SingleInstanceGuard {
        pub fn acquire() -> Self {
            Self
        }
        pub fn check_activation(&self) -> bool {
            false
        }
    }
}

pub use platform::SingleInstanceGuard;
