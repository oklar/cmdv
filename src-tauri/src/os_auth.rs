//! Asks the operating system to confirm that the person at the computer is the signed-in user
//! (Windows Hello: PIN, fingerprint or face) before the app reveals a secret.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OsAuth {
    /// The user confirmed their identity.
    Verified,
    /// The user dismissed the prompt or failed it.
    Cancelled,
    /// This device has no OS prompt the app can use (no Windows Hello set up, or not Windows).
    Unavailable,
}

/// Shows the OS identity prompt over `window` and blocks until the user answers.
/// Call it off the main thread.
#[cfg(windows)]
pub fn verify_user(window: isize, message: &str) -> Result<OsAuth, String> {
    use windows::core::{factory, HSTRING};
    use windows::Security::Credentials::UI::{
        UserConsentVerificationResult, UserConsentVerifier, UserConsentVerifierAvailability,
    };
    use windows::Win32::Foundation::HWND;
    use windows::Win32::System::WinRT::IUserConsentVerifierInterop;
    use windows_future::IAsyncOperation;

    let availability = UserConsentVerifier::CheckAvailabilityAsync()
        .and_then(|op| op.get())
        .map_err(|e| format!("Windows Hello check failed: {e}"))?;
    if availability != UserConsentVerifierAvailability::Available {
        return Ok(OsAuth::Unavailable);
    }

    // The interop variant parents the prompt to our window, so it opens on top of it.
    let interop = factory::<UserConsentVerifier, IUserConsentVerifierInterop>()
        .map_err(|e| format!("Windows Hello is not available: {e}"))?;
    let operation: IAsyncOperation<UserConsentVerificationResult> = unsafe {
        interop.RequestVerificationForWindowAsync(
            HWND(window as *mut core::ffi::c_void),
            &HSTRING::from(message),
        )
    }
    .map_err(|e| format!("Windows Hello prompt failed: {e}"))?;
    let result = operation
        .get()
        .map_err(|e| format!("Windows Hello prompt failed: {e}"))?;

    Ok(match result {
        UserConsentVerificationResult::Verified => OsAuth::Verified,
        UserConsentVerificationResult::DeviceNotPresent
        | UserConsentVerificationResult::NotConfiguredForUser
        | UserConsentVerificationResult::DisabledByPolicy => OsAuth::Unavailable,
        _ => OsAuth::Cancelled,
    })
}

/// Linux has no per-user prompt an app can rely on without installing a polkit policy.
#[cfg(not(windows))]
pub fn verify_user(_window: isize, _message: &str) -> Result<OsAuth, String> {
    Ok(OsAuth::Unavailable)
}

#[cfg(all(test, windows))]
mod tests {
    /// Reaches Windows Hello without showing a prompt: proves the WinRT calls link and run.
    /// Ignored by default because it depends on the machine's Windows Hello setup.
    #[test]
    #[ignore]
    fn windows_hello_availability_can_be_read() {
        use windows::Security::Credentials::UI::UserConsentVerifier;
        let availability = UserConsentVerifier::CheckAvailabilityAsync()
            .and_then(|op| op.get())
            .expect("CheckAvailabilityAsync");
        println!("Windows Hello availability: {}", availability.0);
    }
}
