/// Single-instance enforcement using a Windows Named Mutex and Named Event.
///
/// - The mutex prevents multiple TaskPilot processes from running simultaneously.
/// - The event allows a second instance to signal the first to restore its window,
///   so the first instance uses its own viewport commands (no fragile cross-process
///   window manipulation).

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

    /// Open the activation event owned by the first instance and signal it.
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
