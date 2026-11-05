//! Small OS helpers: free memory and a kill-on-close job object so the engine
//! process never outlives Upscalee (even if Upscalee crashes).

/// Available physical memory in bytes, if the OS reports it.
#[cfg(windows)]
pub fn available_memory_bytes() -> Option<u64> {
    use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};

    // SAFETY: MEMORYSTATUSEX is plain data; dwLength must be set before the call.
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            Some(status.ullAvailPhys)
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub fn available_memory_bytes() -> Option<u64> {
    None
}

/// Put a child process into a process-wide job that kills it when Upscalee exits.
#[cfg(windows)]
pub fn bind_child_to_app_lifetime(child_handle: std::os::windows::io::RawHandle) {
    use std::sync::OnceLock;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation, SetInformationJobObject,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    };

    struct Job(isize);
    static JOB: OnceLock<Option<Job>> = OnceLock::new();

    let job = JOB.get_or_init(|| {
        // SAFETY: creating an anonymous job object and configuring it with a
        // fully initialised limit structure.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            let ok = SetInformationJobObject(
                handle,
                JobObjectExtendedLimitInformation,
                &info as *const _ as *const core::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            if ok == 0 {
                return None;
            }
            // The handle intentionally lives for the whole process.
            Some(Job(handle as isize))
        }
    });

    if let Some(Job(handle)) = job {
        // SAFETY: both handles are valid; failure is harmless (child just isn't bound).
        unsafe {
            AssignProcessToJobObject(*handle as _, child_handle as _);
        }
    }
}

#[cfg(not(windows))]
pub fn bind_child_to_app_lifetime(_child: ()) {}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(windows)]
    fn reports_some_free_memory() {
        let bytes = super::available_memory_bytes().expect("GlobalMemoryStatusEx");
        assert!(bytes > 16 * 1024 * 1024);
    }
}
