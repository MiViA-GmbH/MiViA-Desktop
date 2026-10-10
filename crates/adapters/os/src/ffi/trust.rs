use std::ffi::CStr;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr::{null, null_mut};
use std::slice;

use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::Cryptography::{
    CERT_CHAIN_CONTEXT, CERT_CONTEXT, CERT_NAME_SIMPLE_DISPLAY_TYPE, CTL_USAGE,
    CertGetEnhancedKeyUsage, CertGetNameStringW,
};
use windows_sys::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_WHOLECHAIN,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvCertFromChain,
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};

use super::check;
use crate::trust::{AuthenticodeError, SignerFacts};

pub(crate) fn signer_facts(file: &File, path: &Path) -> Result<SignerFacts, AuthenticodeError> {
    let path: Vec<u16> = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    let mut info = WINTRUST_FILE_INFO {
        cbStruct: byte_len::<WINTRUST_FILE_INFO>(),
        pcwszFilePath: path.as_ptr(),
        hFile: file.as_raw_handle(),
        pgKnownSubject: null_mut(),
    };
    let mut data = WINTRUST_DATA {
        cbStruct: byte_len::<WINTRUST_DATA>(),
        dwUIChoice: WTD_UI_NONE,
        fdwRevocationChecks: WTD_REVOKE_WHOLECHAIN,
        dwUnionChoice: WTD_CHOICE_FILE,
        Anonymous: WINTRUST_DATA_0 {
            pFile: &raw mut info,
        },
        dwStateAction: WTD_STATEACTION_VERIFY,
        dwProvFlags: WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT,
        ..WINTRUST_DATA::default()
    };
    let mut action = WINTRUST_ACTION_GENERIC_VERIFY_V2;
    // SAFETY: the data, file info and path buffer are live locals for both calls.
    let status = unsafe { WinVerifyTrust(null_mut(), &raw mut action, (&raw mut data).cast()) };
    let facts = if status == 0 {
        read_facts(data.hWVTStateData)
    } else {
        Err(AuthenticodeError::Untrusted(status))
    };
    data.dwStateAction = WTD_STATEACTION_CLOSE;
    // SAFETY: releases the state data the verify call allocated, with the same live locals.
    unsafe { WinVerifyTrust(null_mut(), &raw mut action, (&raw mut data).cast()) };
    facts
}

fn read_facts(state: HANDLE) -> Result<SignerFacts, AuthenticodeError> {
    // SAFETY: the state handle comes from a successful verify and is still open.
    let provider = unsafe { WTHelperProvDataFromStateData(state) };
    if provider.is_null() {
        return Err(AuthenticodeError::Io);
    }
    // SAFETY: the provider data is non-null and owned by the open state.
    let signer_ptr = unsafe { WTHelperGetProvSignerFromChain(provider, 0, 0, 0) };
    // SAFETY: a non-null signer lives as long as the open state.
    let signer = unsafe { signer_ptr.as_ref() }.ok_or(AuthenticodeError::Io)?;
    // SAFETY: the signer is non-null; a non-null certificate lives as long as the open state.
    let leaf = unsafe { WTHelperGetProvCertFromChain(signer_ptr, 0).as_ref() }
        .ok_or(AuthenticodeError::Io)?;
    Ok(SignerFacts {
        timestamped: signer.csCounterSigners > 0,
        ekus: ekus(leaf.pCert)?,
        root: root_name(signer.pChainContext)?,
    })
}

fn ekus(cert: *const CERT_CONTEXT) -> Result<Vec<String>, AuthenticodeError> {
    if cert.is_null() {
        return Err(AuthenticodeError::Io);
    }
    let mut size = 0_u32;
    // SAFETY: a size query with a null buffer on a live certificate.
    check(unsafe { CertGetEnhancedKeyUsage(cert, 0, null_mut(), &raw mut size) })
        .map_err(|_| AuthenticodeError::Io)?;
    let mut buffer = vec![0_u64; usize::try_from(size).unwrap_or_default().div_ceil(8)];
    let usage = buffer.as_mut_ptr().cast::<CTL_USAGE>();
    // SAFETY: the aligned buffer holds at least `size` bytes.
    check(unsafe { CertGetEnhancedKeyUsage(cert, 0, usage, &raw mut size) })
        .map_err(|_| AuthenticodeError::Io)?;
    // SAFETY: the call above filled a CTL_USAGE at the start of the buffer.
    let usage = unsafe { &*usage };
    // SAFETY: the identifier array has `cUsageIdentifier` entries inside the buffer.
    let ids = unsafe { items(usage.rgpszUsageIdentifier, usage.cUsageIdentifier) };
    Ok(ids
        .iter()
        .filter(|id| !id.is_null())
        // SAFETY: each non-null identifier is a NUL-terminated OID string inside the buffer.
        .map(|&id| unsafe { CStr::from_ptr(id.cast()) }.to_string_lossy().into_owned())
        .collect())
}

fn root_name(chain: *const CERT_CHAIN_CONTEXT) -> Result<String, AuthenticodeError> {
    // SAFETY: a non-null chain lives as long as the open state.
    let chain = unsafe { chain.as_ref() }.ok_or(AuthenticodeError::Io)?;
    // SAFETY: the chain array has `cChain` entries; each non-null entry is live.
    let simple = unsafe { items(chain.rgpChain, chain.cChain) }
        .first()
        // SAFETY: a non-null simple chain lives as long as the open state.
        .and_then(|&simple| unsafe { simple.as_ref() })
        .ok_or(AuthenticodeError::Io)?;
    // SAFETY: the element array has `cElement` entries; each non-null entry is live.
    let root = unsafe { items(simple.rgpElement, simple.cElement) }
        .last()
        // SAFETY: a non-null element lives as long as the open state.
        .and_then(|&element| unsafe { element.as_ref() })
        .ok_or(AuthenticodeError::Io)?;
    if root.pCertContext.is_null() {
        return Err(AuthenticodeError::Io);
    }
    Ok(simple_name(root.pCertContext))
}

fn simple_name(cert: *const CERT_CONTEXT) -> String {
    // SAFETY: a length query with a null buffer on a live certificate.
    let len = unsafe {
        CertGetNameStringW(
            cert,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            null(),
            null_mut(),
            0,
        )
    };
    let mut buffer = vec![0_u16; usize::try_from(len).unwrap_or_default()];
    // SAFETY: the buffer holds `len` UTF-16 units.
    let written = unsafe {
        CertGetNameStringW(
            cert,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            0,
            null(),
            buffer.as_mut_ptr(),
            len,
        )
    };
    buffer.truncate(
        usize::try_from(written)
            .unwrap_or_default()
            .saturating_sub(1),
    );
    String::from_utf16_lossy(&buffer)
}

unsafe fn items<'a, T>(start: *const T, count: u32) -> &'a [T] {
    if start.is_null() {
        return &[];
    }
    // SAFETY: the caller passes a Win32 array pointer with its element count.
    unsafe { slice::from_raw_parts(start, usize::try_from(count).unwrap_or_default()) }
}

fn byte_len<T>() -> u32 {
    u32::try_from(size_of::<T>()).unwrap_or(u32::MAX)
}
