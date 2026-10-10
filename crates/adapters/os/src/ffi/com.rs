use std::io;
use std::path::Path;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
    CoTaskMemFree, CoUninitialize, IPersistFile, STGM_READ,
};
use windows::Win32::UI::Shell::Common::COMDLG_FILTERSPEC;
use windows::Win32::UI::Shell::{
    EXP_SZ_LINK, EXP_SZ_LINK_SIG, FOS_ALLOWMULTISELECT, FOS_FILEMUSTEXIST, FOS_FORCEFILESYSTEM,
    FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, IShellItem, IShellLinkDataList, IShellLinkW,
    SIGDN_FILESYSPATH, SLDF_HAS_EXP_SZ, SLGP_RAWPATH, ShellLink,
};
use windows::core::{HSTRING, Interface, PCWSTR, PWSTR};

const MAX_LINK_TEXT: usize = 1024;
const LINK_TARGET_CHARS: usize = 260;

struct Apartment(bool);

impl Apartment {
    fn enter() -> Self {
        // SAFETY: initialises COM for this thread; balanced in Drop when it succeeded.
        Self(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful CoInitializeEx on the same thread.
            unsafe { CoUninitialize() };
        }
    }
}

fn create<T: Interface>(class: &windows::core::GUID) -> io::Result<T> {
    // SAFETY: creates an in-process COM object on a thread with an initialised apartment.
    unsafe { CoCreateInstance(class, None, CLSCTX_INPROC_SERVER) }.map_err(io::Error::from)
}

fn text(buffer: &[u16]) -> String {
    let end = buffer.iter().position(|c| *c == 0).unwrap_or(buffer.len());
    String::from_utf16_lossy(buffer.get(..end).unwrap_or_default())
}

fn expandable_block(target: &str) -> io::Result<EXP_SZ_LINK> {
    let mut wide = [0_u16; LINK_TARGET_CHARS];
    let mut narrow = [0_i8; LINK_TARGET_CHARS];
    if target.len() >= LINK_TARGET_CHARS || !target.is_ascii() {
        return Err(io::Error::from(io::ErrorKind::InvalidInput));
    }
    wide.iter_mut()
        .zip(target.encode_utf16())
        .for_each(|(slot, unit)| *slot = unit);
    narrow
        .iter_mut()
        .zip(target.bytes())
        .for_each(|(slot, byte)| *slot = byte.cast_signed());
    Ok(EXP_SZ_LINK {
        cbSize: u32::try_from(size_of::<EXP_SZ_LINK>()).unwrap_or(u32::MAX),
        dwSignature: EXP_SZ_LINK_SIG,
        szTarget: narrow,
        swzTarget: wide,
    })
}

pub fn save_link(file: &Path, target: &str, arguments: &str) -> io::Result<()> {
    let _apartment = Apartment::enter();
    let link: IShellLinkW = create(&ShellLink)?;
    let expanded = super::expand_environment(target);
    // SAFETY: the link is a live COM object and every string and block outlives its call.
    unsafe {
        link.SetPath(&HSTRING::from(expanded.as_str()))?;
        link.SetArguments(&HSTRING::from(arguments))?;
        link.SetIconLocation(&HSTRING::from(target), 0)?;
        if expanded != target {
            let block = expandable_block(target)?;
            let data = link.cast::<IShellLinkDataList>()?;
            data.AddDataBlock((&raw const block).cast())?;
            data.SetFlags(data.GetFlags()? | SLDF_HAS_EXP_SZ.0.cast_unsigned())?;
        }
        link.cast::<IPersistFile>()?
            .Save(&HSTRING::from(file.as_os_str()), true)?;
    }
    Ok(())
}

pub fn read_link(file: &Path) -> io::Result<(String, String)> {
    let _apartment = Apartment::enter();
    let link: IShellLinkW = create(&ShellLink)?;
    let mut target = vec![0_u16; MAX_LINK_TEXT];
    let mut arguments = vec![0_u16; MAX_LINK_TEXT];
    // SAFETY: the buffers are sized locals and the file name outlives the Load call.
    unsafe {
        link.cast::<IPersistFile>()?
            .Load(&HSTRING::from(file.as_os_str()), STGM_READ)?;
        link.GetPath(
            &mut target,
            std::ptr::null_mut(),
            SLGP_RAWPATH.0.cast_unsigned(),
        )?;
        link.GetArguments(&mut arguments)?;
    }
    Ok((text(&target), text(&arguments)))
}

fn item_path(item: &IShellItem) -> io::Result<String> {
    // SAFETY: the item is a live COM object; the returned path is freed with CoTaskMemFree.
    unsafe {
        let name: PWSTR = item.GetDisplayName(SIGDN_FILESYSPATH)?;
        let path = name
            .to_string()
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidData));
        CoTaskMemFree(Some(name.0.cast_const().cast()));
        path
    }
}

pub fn pick_folder(owner: isize, title: &str) -> io::Result<Option<String>> {
    let _apartment = Apartment::enter();
    let dialog: IFileOpenDialog = create(&FileOpenDialog)?;
    // SAFETY: the dialog is a live COM object used on the thread that created it.
    unsafe {
        dialog.SetOptions(dialog.GetOptions()? | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)?;
        dialog.SetTitle(&HSTRING::from(title))?;
        if dialog.Show(Some(HWND(owner as *mut _))).is_err() {
            return Ok(None);
        }
        item_path(&dialog.GetResult()?).map(Some)
    }
}

pub fn pick_files(owner: isize, filter: (&str, &str)) -> io::Result<Vec<String>> {
    let _apartment = Apartment::enter();
    let dialog: IFileOpenDialog = create(&FileOpenDialog)?;
    let (name, spec) = (HSTRING::from(filter.0), HSTRING::from(filter.1));
    let types = [COMDLG_FILTERSPEC {
        pszName: PCWSTR(name.as_ptr()),
        pszSpec: PCWSTR(spec.as_ptr()),
    }];
    // SAFETY: a live COM object on its creating thread; the filter strings outlive the call.
    unsafe {
        let options = FOS_ALLOWMULTISELECT | FOS_FILEMUSTEXIST | FOS_FORCEFILESYSTEM;
        dialog.SetOptions(dialog.GetOptions()? | options)?;
        dialog.SetFileTypes(&types)?;
        if dialog.Show(Some(HWND(owner as *mut _))).is_err() {
            return Ok(Vec::new());
        }
        let items = dialog.GetResults()?;
        (0..items.GetCount()?)
            .map(|index| item_path(&items.GetItemAt(index)?))
            .collect()
    }
}
