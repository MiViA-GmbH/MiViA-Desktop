use std::ffi::CStr;
use std::fs::File;
use std::os::windows::ffi::OsStrExt;
use std::os::windows::io::AsRawHandle;
use std::path::Path;
use std::ptr::null_mut;
use std::slice;

use mivia_domain::ids::Fingerprint;
use sha2::{Digest, Sha256};
use windows_sys::Win32::Foundation::HANDLE;
use windows_sys::Win32::Security::Cryptography::{
    CERT_CONTEXT, CERT_NAME_ATTR_TYPE, CTL_USAGE, CertGetEnhancedKeyUsage, CertGetNameStringW,
    CryptEncodeObject, X509_ASN_ENCODING, X509_PUBLIC_KEY_INFO, szOID_COMMON_NAME,
    szOID_ORGANIZATION_NAME,
};
use windows_sys::Win32::Security::WinTrust::CRYPT_PROVIDER_SGNR;
use windows_sys::Win32::Security::WinTrust::{
    WINTRUST_ACTION_GENERIC_VERIFY_V2, WINTRUST_DATA, WINTRUST_DATA_0, WINTRUST_FILE_INFO,
    WTD_CHOICE_FILE, WTD_REVOCATION_CHECK_CHAIN_EXCLUDE_ROOT, WTD_REVOKE_WHOLECHAIN,
    WTD_STATEACTION_CLOSE, WTD_STATEACTION_VERIFY, WTD_UI_NONE, WTHelperGetProvCertFromChain,
    WTHelperGetProvSignerFromChain, WTHelperProvDataFromStateData, WinVerifyTrust,
};
use windows_sys::core::PCSTR;

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
    let leaf = cert_at(signer_ptr, 0)?;
    Ok(SignerFacts {
        timestamped: signer.csCounterSigners > 0,
        ekus: ekus(leaf)?,
        organization: name_attribute(leaf, szOID_ORGANIZATION_NAME),
        common_name: name_attribute(leaf, szOID_COMMON_NAME),
        intermediate_key: key_hash(cert_at(signer_ptr, 1)?)?,
    })
}

fn cert_at(
    signer: *mut CRYPT_PROVIDER_SGNR,
    index: u32,
) -> Result<*const CERT_CONTEXT, AuthenticodeError> {
    // SAFETY: the signer is non-null and owned by the open state; out-of-range indexes return null.
    let entry = unsafe { WTHelperGetProvCertFromChain(signer, index).as_ref() }
        .ok_or(AuthenticodeError::Io)?;
    if entry.pCert.is_null() {
        return Err(AuthenticodeError::Io);
    }
    Ok(entry.pCert)
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

fn key_hash(cert: *const CERT_CONTEXT) -> Result<String, AuthenticodeError> {
    // SAFETY: a non-null certificate and its info live as long as the open state.
    let info = unsafe { cert.as_ref().and_then(|cert| cert.pCertInfo.as_ref()) }
        .ok_or(AuthenticodeError::Io)?;
    let key = (&raw const info.SubjectPublicKeyInfo).cast();
    let mut size = 0_u32;
    // SAFETY: a size query with a null buffer on the live public key info.
    check(unsafe {
        CryptEncodeObject(
            X509_ASN_ENCODING,
            X509_PUBLIC_KEY_INFO,
            key,
            null_mut(),
            &raw mut size,
        )
    })
    .map_err(|_| AuthenticodeError::Io)?;
    let mut der = vec![0_u8; usize::try_from(size).unwrap_or_default()];
    // SAFETY: the buffer holds `size` bytes.
    check(unsafe {
        CryptEncodeObject(
            X509_ASN_ENCODING,
            X509_PUBLIC_KEY_INFO,
            key,
            der.as_mut_ptr(),
            &raw mut size,
        )
    })
    .map_err(|_| AuthenticodeError::Io)?;
    der.truncate(usize::try_from(size).unwrap_or_default());
    Ok(Fingerprint(Sha256::digest(&der).into()).to_string())
}

fn name_attribute(cert: *const CERT_CONTEXT, oid: PCSTR) -> String {
    // SAFETY: a length query with a null buffer on a live certificate.
    let len =
        unsafe { CertGetNameStringW(cert, CERT_NAME_ATTR_TYPE, 0, oid.cast(), null_mut(), 0) };
    let mut buffer = vec![0_u16; usize::try_from(len).unwrap_or_default()];
    // SAFETY: the buffer holds `len` UTF-16 units.
    let written = unsafe {
        CertGetNameStringW(
            cert,
            CERT_NAME_ATTR_TYPE,
            0,
            oid.cast(),
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
