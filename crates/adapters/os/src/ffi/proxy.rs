use std::ffi::c_void;
use std::ptr::{null, null_mut};
use std::time::Duration;

use mivia_ports::network::ProxyEntry;
use parking_lot::Mutex;
use tokio::sync::oneshot;
use windows_sys::Win32::Foundation::{ERROR_IO_PENDING, GlobalFree};
use windows_sys::Win32::Networking::WinHttp::{
    ERROR_WINHTTP_LOGIN_FAILURE, WINHTTP_ACCESS_TYPE_NAMED_PROXY, WINHTTP_ACCESS_TYPE_NO_PROXY,
    WINHTTP_ASYNC_RESULT, WINHTTP_AUTO_DETECT_TYPE_DHCP, WINHTTP_AUTO_DETECT_TYPE_DNS_A,
    WINHTTP_AUTOPROXY_AUTO_DETECT, WINHTTP_AUTOPROXY_CONFIG_URL, WINHTTP_AUTOPROXY_OPTIONS,
    WINHTTP_CALLBACK_FLAG_GETPROXYFORURL_COMPLETE, WINHTTP_CALLBACK_FLAG_REQUEST_ERROR,
    WINHTTP_CALLBACK_STATUS_GETPROXYFORURL_COMPLETE, WINHTTP_CALLBACK_STATUS_HANDLE_CLOSING,
    WINHTTP_CALLBACK_STATUS_REQUEST_ERROR, WINHTTP_CURRENT_USER_IE_PROXY_CONFIG,
    WINHTTP_FLAG_ASYNC, WINHTTP_INTERNET_SCHEME_HTTP, WINHTTP_PROXY_INFO, WINHTTP_PROXY_RESULT,
    WinHttpCloseHandle, WinHttpCreateProxyResolver, WinHttpFreeProxyResult,
    WinHttpGetDefaultProxyConfiguration, WinHttpGetIEProxyConfigForCurrentUser,
    WinHttpGetProxyForUrlEx, WinHttpGetProxyResult, WinHttpOpen, WinHttpSetStatusCallback,
};
use windows_sys::Win32::Security::Cryptography::{
    CERT_NAME_ISSUER_FLAG, CERT_NAME_SIMPLE_DISPLAY_TYPE, CertCreateCertificateContext,
    CertFreeCertificateContext, CertGetNameStringW, X509_ASN_ENCODING,
};
use windows_sys::core::PWSTR;

use super::system::utf16_until_nul;
use super::wide;

const TIMEOUT: Duration = Duration::from_secs(15);
const MAX_CHARS: usize = 32 * 1024;
const AGENT: &str = "MiViA-Desktop";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct IeProxyConfig {
    pub(crate) auto_detect: bool,
    pub(crate) pac_url: Option<String>,
    pub(crate) proxy: Option<String>,
    pub(crate) bypass: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Logon {
    Never,
    Automatic,
}

type Answer = Result<Vec<ProxyEntry>, u32>;

struct Slot {
    sender: Mutex<Option<oneshot::Sender<Answer>>>,
    url: Vec<u16>,
    pac: Option<Vec<u16>>,
}

struct Handle(*mut c_void);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: the handle came from WinHttpOpen or WinHttpCreateProxyResolver and is closed once.
        unsafe { WinHttpCloseHandle(self.0) };
    }
}

// SAFETY: a WinHTTP handle may be closed from any thread.
unsafe impl Send for Handle {}

fn take_string(text: PWSTR) -> Option<String> {
    if text.is_null() {
        return None;
    }
    let value = utf16_until_nul(text, MAX_CHARS);
    // SAFETY: WinHTTP allocated the string with GlobalAlloc and hands ownership to the caller.
    unsafe { GlobalFree(text.cast()) };
    Some(value).filter(|value| !value.trim().is_empty())
}

pub(crate) fn user_proxy_config() -> Option<IeProxyConfig> {
    let mut config = WINHTTP_CURRENT_USER_IE_PROXY_CONFIG::default();
    // SAFETY: the out struct is a live local; its strings are freed by take_string.
    if unsafe { WinHttpGetIEProxyConfigForCurrentUser(&raw mut config) } == 0 {
        return None;
    }
    Some(IeProxyConfig {
        auto_detect: config.fAutoDetect != 0,
        pac_url: take_string(config.lpszAutoConfigUrl),
        proxy: take_string(config.lpszProxy),
        bypass: take_string(config.lpszProxyBypass),
    })
}

pub(crate) fn machine_proxy_config() -> Option<(String, Option<String>)> {
    let mut info = WINHTTP_PROXY_INFO::default();
    // SAFETY: the out struct is a live local; its strings are freed by take_string.
    if unsafe { WinHttpGetDefaultProxyConfiguration(&raw mut info) } == 0 {
        return None;
    }
    let proxy = take_string(info.lpszProxy);
    let bypass = take_string(info.lpszProxyBypass);
    let named = info.dwAccessType == WINHTTP_ACCESS_TYPE_NAMED_PROXY;
    proxy.filter(|_| named).map(|proxy| (proxy, bypass))
}

fn read_result(resolver: *mut c_void) -> Answer {
    let mut result = WINHTTP_PROXY_RESULT::default();
    // SAFETY: the resolver handle is the one the completion was reported for.
    let error = unsafe { WinHttpGetProxyResult(resolver, &raw mut result) };
    if error != 0 {
        return Err(error);
    }
    let list = entries_of(&result);
    // SAFETY: frees the result filled by WinHttpGetProxyResult above.
    unsafe { WinHttpFreeProxyResult(&raw mut result) };
    Ok(list)
}

fn entries_of(result: &WINHTTP_PROXY_RESULT) -> Vec<ProxyEntry> {
    let count = usize::try_from(result.cEntries).unwrap_or_default();
    if result.pEntries.is_null() || count == 0 {
        return Vec::new();
    }
    // SAFETY: WinHTTP returned cEntries valid entries at pEntries.
    let entries = unsafe { std::slice::from_raw_parts(result.pEntries, count) };
    entries
        .iter()
        .filter_map(|entry| {
            if entry.fProxy == 0 {
                return Some(ProxyEntry::Direct);
            }
            let http = entry.ProxyScheme == WINHTTP_INTERNET_SCHEME_HTTP;
            let host = (http && !entry.pwszProxy.is_null())
                .then(|| utf16_until_nul(entry.pwszProxy, MAX_CHARS))?;
            Some(ProxyEntry::Http(format!("{host}:{}", entry.ProxyPort)))
        })
        .collect()
}

unsafe extern "system" fn callback(
    handle: *mut c_void,
    context: usize,
    status: u32,
    info: *mut c_void,
    _length: u32,
) {
    if context == 0 {
        return;
    }
    let slot = context as *mut Slot;
    let answer = match status {
        WINHTTP_CALLBACK_STATUS_GETPROXYFORURL_COMPLETE => read_result(handle),
        WINHTTP_CALLBACK_STATUS_REQUEST_ERROR if !info.is_null() => {
            // SAFETY: for REQUEST_ERROR the status information is a WINHTTP_ASYNC_RESULT.
            Err(unsafe { (*info.cast::<WINHTTP_ASYNC_RESULT>()).dwError })
        }
        WINHTTP_CALLBACK_STATUS_HANDLE_CLOSING => {
            // SAFETY: the context was leaked from a Box in lookup and this is its last notification.
            drop(unsafe { Box::from_raw(slot) });
            return;
        }
        _ => return,
    };
    // SAFETY: the slot stays alive until HANDLE_CLOSING, which comes after this notification.
    let sender = unsafe { (*slot).sender.lock().take() };
    if let Some(sender) = sender {
        drop(sender.send(answer));
    }
}

fn options(pac: Option<&[u16]>, detect: bool, logon: Logon) -> WINHTTP_AUTOPROXY_OPTIONS {
    let detect_flag = if detect {
        WINHTTP_AUTOPROXY_AUTO_DETECT
    } else {
        0
    };
    let url_flag = if pac.is_some() {
        WINHTTP_AUTOPROXY_CONFIG_URL
    } else {
        0
    };
    let types = if detect {
        WINHTTP_AUTO_DETECT_TYPE_DHCP | WINHTTP_AUTO_DETECT_TYPE_DNS_A
    } else {
        0
    };
    WINHTTP_AUTOPROXY_OPTIONS {
        dwFlags: detect_flag | url_flag,
        dwAutoDetectFlags: types,
        lpszAutoConfigUrl: pac.map_or(null(), <[u16]>::as_ptr),
        lpvReserved: null_mut(),
        dwReserved: 0,
        fAutoLogonIfChallenged: i32::from(logon == Logon::Automatic),
    }
}

fn open() -> Option<Handle> {
    let agent = wide(AGENT);
    // SAFETY: the agent string is NUL-terminated and outlives the call.
    let session = unsafe {
        WinHttpOpen(
            agent.as_ptr(),
            WINHTTP_ACCESS_TYPE_NO_PROXY,
            null(),
            null(),
            WINHTTP_FLAG_ASYNC,
        )
    };
    (!session.is_null()).then(|| Handle(session))
}

type Started = (Handle, Handle, oneshot::Receiver<Answer>);

fn start(url: &str, detect: bool, pac_url: Option<&str>, logon: Logon) -> Result<Started, u32> {
    let session = open().ok_or(0_u32)?;
    let flags = WINHTTP_CALLBACK_FLAG_GETPROXYFORURL_COMPLETE
        | WINHTTP_CALLBACK_FLAG_REQUEST_ERROR
        | WINHTTP_CALLBACK_STATUS_HANDLE_CLOSING;
    // SAFETY: the session is open and the callback matches WINHTTP_STATUS_CALLBACK.
    unsafe { WinHttpSetStatusCallback(session.0, Some(callback), flags, 0) };
    let mut raw = null_mut();
    // SAFETY: the session is a live async session; the out pointer is a live local.
    let created = unsafe { WinHttpCreateProxyResolver(session.0, &raw mut raw) };
    if created != 0 {
        return Err(created);
    }
    let resolver = Handle(raw);
    let (sender, receiver) = oneshot::channel();
    let slot = Box::new(Slot {
        sender: Mutex::new(Some(sender)),
        url: wide(url),
        pac: pac_url.map(wide),
    });
    let options = options(slot.pac.as_deref(), detect, logon);
    let url_ptr = slot.url.as_ptr();
    let context = Box::into_raw(slot) as usize;
    // SAFETY: the url and PAC strings live in the slot, freed only at HANDLE_CLOSING.
    let error =
        unsafe { WinHttpGetProxyForUrlEx(resolver.0, url_ptr, &raw const options, context) };
    if error != ERROR_IO_PENDING {
        return Err(error);
    }
    Ok((session, resolver, receiver))
}

async fn lookup_once(url: &str, detect: bool, pac_url: Option<&str>, logon: Logon) -> Answer {
    let (_session, _resolver, receiver) = start(url, detect, pac_url, logon)?;
    match tokio::time::timeout(TIMEOUT, receiver).await {
        Ok(Ok(answer)) => answer,
        Ok(Err(_)) | Err(_) => Err(u32::MAX),
    }
}

pub async fn proxy_for_url(url: &str, detect: bool, pac_url: Option<&str>) -> Answer {
    match lookup_once(url, detect, pac_url, Logon::Never).await {
        Err(ERROR_WINHTTP_LOGIN_FAILURE) => {
            lookup_once(url, detect, pac_url, Logon::Automatic).await
        }
        other => other,
    }
}

pub fn certificate_issuer(der: &[u8]) -> Option<String> {
    let length = u32::try_from(der.len()).ok()?;
    // SAFETY: the pointer and length describe the caller's DER buffer; Windows copies it.
    let context = unsafe { CertCreateCertificateContext(X509_ASN_ENCODING, der.as_ptr(), length) };
    if context.is_null() {
        return None;
    }
    let mut buffer = vec![0_u16; 256];
    let capacity = u32::try_from(buffer.len()).unwrap_or_default();
    // SAFETY: the context is live and the buffer holds `capacity` u16 values.
    let written = unsafe {
        CertGetNameStringW(
            context,
            CERT_NAME_SIMPLE_DISPLAY_TYPE,
            CERT_NAME_ISSUER_FLAG,
            null(),
            buffer.as_mut_ptr(),
            capacity,
        )
    };
    // SAFETY: frees the context created above.
    unsafe { CertFreeCertificateContext(context) };
    let name = utf16_until_nul(
        buffer.as_ptr(),
        usize::try_from(written).unwrap_or_default(),
    );
    Some(name).filter(|name| !name.is_empty())
}
