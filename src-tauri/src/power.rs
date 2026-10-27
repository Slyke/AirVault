//! Windows power notifications invalidate the session before any subsequent IPC action.
use airvault_core::session::Session;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, OnceLock, Weak,
};
use windows_sys::Win32::System::Power::{
    PowerRegisterSuspendResumeNotification, DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS,
};
static SESSION: OnceLock<Weak<Mutex<Session>>> = OnceLock::new();
static MUST_LOCK: AtomicBool = AtomicBool::new(false);
pub fn take_lock_request() -> bool {
    MUST_LOCK.swap(false, Ordering::AcqRel)
}
unsafe extern "system" fn notification(
    _: *const core::ffi::c_void,
    _: u32,
    _: *const core::ffi::c_void,
) -> u32 {
    MUST_LOCK.store(true, Ordering::Release);
    if let Some(session) = SESSION.get().and_then(Weak::upgrade) {
        if let Ok(mut session) = session.try_lock() {
            session.lock();
        }
    }
    0
}
pub fn register(session: &Arc<Mutex<Session>>) -> std::io::Result<()> {
    SESSION
        .set(Arc::downgrade(session))
        .map_err(|_| std::io::Error::other("power notifications already registered"))?;
    // The callback descriptor and registration live until process exit. No secret is stored
    // in the descriptor; the static session reference is weak. Keep the descriptor stable.
    let parameters = Box::into_raw(Box::new(DEVICE_NOTIFY_SUBSCRIBE_PARAMETERS {
        Callback: Some(notification),
        Context: std::ptr::null_mut(),
    }));
    let mut handle = std::ptr::null_mut();
    // DEVICE_NOTIFY_CALLBACK = 2. The API requires the descriptor cast to HANDLE.
    let status =
        unsafe { PowerRegisterSuspendResumeNotification(2, parameters.cast(), &mut handle) };
    if status != 0 {
        unsafe {
            drop(Box::from_raw(parameters));
        }
        return Err(std::io::Error::from_raw_os_error(status as i32));
    }
    Ok(())
}
