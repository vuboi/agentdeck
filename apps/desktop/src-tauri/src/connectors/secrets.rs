//! Shared credential-store access for connectors and provider API keys.

/// What a secret of the system store holds (Windows Credential Manager), as text.
#[cfg(windows)]
pub(crate) fn stored_secret(target: &str) -> Option<String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{
        CredFree, CredReadW, CREDENTIALW, CRED_TYPE_GENERIC,
    };

    let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let mut found: *mut CREDENTIALW = std::ptr::null_mut();
    // SAFETY: CredReadW allocates `found`; it is copied once and released with CredFree.
    let bytes = unsafe {
        CredReadW(PCWSTR(wide.as_ptr()), CRED_TYPE_GENERIC, None, &mut found).ok()?;
        let credential = &*found;
        let bytes = if credential.CredentialBlob.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(
                credential.CredentialBlob,
                credential.CredentialBlobSize as usize,
            )
            .to_vec()
        };
        CredFree(found.cast());
        bytes
    };
    match String::from_utf8(bytes.clone()) {
        Ok(text) if text.trim_start().starts_with('{') => Some(text),
        _ => {
            let units: Vec<u16> = bytes
                .chunks_exact(2)
                .map(|pair| u16::from_le_bytes([pair[0], pair[1]]))
                .collect();
            String::from_utf16(&units).ok()
        }
    }
}

/// Writes a credential into Windows Credential Manager; callers keep tokens out of files and logs.
#[cfg(windows)]
pub(crate) fn write_secret(target: &str, secret: &str) -> Result<(), String> {
    use windows::core::PWSTR;
    use windows::Win32::Security::Credentials::{
        CredWriteW, CREDENTIALW, CRED_PERSIST_LOCAL_MACHINE, CRED_TYPE_GENERIC,
    };

    let mut target_wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    let mut bytes = secret.as_bytes().to_vec();
    let credential = CREDENTIALW {
        Type: CRED_TYPE_GENERIC,
        TargetName: PWSTR(target_wide.as_mut_ptr()),
        CredentialBlobSize: u32::try_from(bytes.len()).map_err(|error| error.to_string())?,
        CredentialBlob: bytes.as_mut_ptr(),
        Persist: CRED_PERSIST_LOCAL_MACHINE,
        ..Default::default()
    };
    // SAFETY: target and blob stay alive while CredWriteW copies them.
    unsafe { CredWriteW(&credential, 0).map_err(|error| error.to_string()) }
}

#[cfg(windows)]
pub(crate) fn delete_secret(target: &str) -> Result<(), String> {
    use windows::core::PCWSTR;
    use windows::Win32::Security::Credentials::{CredDeleteW, CRED_TYPE_GENERIC};

    if stored_secret(target).is_none() {
        return Ok(());
    }
    let wide: Vec<u16> = target.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `wide` remains valid for the duration of the Windows API call.
    unsafe {
        CredDeleteW(PCWSTR(wide.as_ptr()), CRED_TYPE_GENERIC, None)
            .map_err(|error| error.to_string())
    }
}

#[cfg(not(windows))]
pub(crate) fn stored_secret(_target: &str) -> Option<String> {
    None
}

#[cfg(not(windows))]
pub(crate) fn write_secret(_target: &str, _secret: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub(crate) fn delete_secret(_target: &str) -> Result<(), String> {
    Ok(())
}

fn secret_target(target: &str) -> Result<String, String> {
    if target.starts_with("agentdeck:")
        && target.len() > "agentdeck:".len()
        && !target.contains('\0')
    {
        Ok(target.to_string())
    } else {
        Err("not an AgentDeck secret".to_string())
    }
}

fn secret_entry(target: &str) -> Result<keyring::Entry, String> {
    keyring::Entry::new("AgentDeck", target).map_err(|error| error.to_string())
}

pub fn secret_save(target: String, value: String) -> Result<(), String> {
    let target = secret_target(&target)?;
    secret_entry(&target)?
        .set_password(&value)
        .map_err(|error| error.to_string())
}

pub fn secret_load(target: String) -> Option<String> {
    let target = secret_target(&target).ok()?;
    secret_entry(&target).ok()?.get_password().ok()
}

pub fn secret_delete(target: String) -> Result<(), String> {
    let target = secret_target(&target)?;
    match secret_entry(&target)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(error.to_string()),
    }
}
