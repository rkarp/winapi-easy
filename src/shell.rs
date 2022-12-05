use windows::Win32::UI::Shell::SHChangeNotify;
use windows::Win32::UI::Shell::SHCNE_ASSOCCHANGED;
use windows::Win32::UI::Shell::SHCNF_IDLIST;

/// Forces a refresh of the Windows icon cache.
pub fn refresh_icon_cache() {
    unsafe {
        SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None);
    }
}
