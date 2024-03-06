//! Multimedia.

use std::ffi::{
    OsStr,
    OsString,
};
use std::io;
use std::iter::once;
use std::os::windows::ffi::{
    OsStrExt,
    OsStringExt,
};

use windows::core::{
    GUID,
    PCWSTR,
};
use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
use windows::Win32::Media::Audio::{
    eConsole,
    eRender,
    IMMDevice,
    IMMDeviceEnumerator,
    MMDeviceEnumerator,
    DEVICE_STATE_ACTIVE,
};
use windows::Win32::System::Com::StructuredStorage::PROPVARIANT;
use windows::Win32::System::Com::STGM_READ;

use crate::com::{
    ComInterfaceExt,
    ComTaskMemory,
};

impl ComInterfaceExt for IMMDeviceEnumerator {
    const CLASS_GUID: GUID = MMDeviceEnumerator;
}

/// A representation of a windows audio output device.
#[derive(Clone, Eq, Debug)]
pub struct AudioOutputDevice {
    id: OsString,
    friendly_name: String,
}

impl AudioOutputDevice {
    /// Returns all devices that are active (currently plugged in)
    pub fn get_active_devices() -> io::Result<Vec<Self>> {
        let enumerator = IMMDeviceEnumerator::new_instance()?;
        let endpoints = unsafe { enumerator.EnumAudioEndpoints(eRender, DEVICE_STATE_ACTIVE) }?;
        let num_endpoints = unsafe { endpoints.GetCount() }?;
        (0..num_endpoints)
            .map(|idx| {
                let item = unsafe { endpoints.Item(idx)? };
                item.try_into()
            })
            .collect()
    }

    /// Returns the internal windows ID.
    pub fn get_id(&self) -> &OsStr {
        &self.id
    }

    /// Returns a friendly name usable for humans to identify the device.
    pub fn get_friendly_name(&self) -> &str {
        &self.friendly_name
    }

    /// Returns the current global default audio output device set in the audio settings.
    pub fn get_global_default() -> io::Result<Self> {
        let enumerator = IMMDeviceEnumerator::new_instance()?;
        let raw_device = unsafe { enumerator.GetDefaultAudioEndpoint(eRender, eConsole) }?;
        raw_device.try_into()
    }

    /// Sets the device as the new default global output device.
    pub fn set_global_default(&self) -> io::Result<()> {
        let policy_config = policy_config::IPolicyConfig::new_instance()?;
        let raw_id: Vec<u16> = self.get_id().encode_wide().chain(once(0)).collect();
        let result = unsafe { policy_config.SetDefaultEndpoint(PCWSTR(raw_id.as_ptr()), eConsole) };
        result.map_err(Into::into)
    }
}

impl TryFrom<IMMDevice> for AudioOutputDevice {
    type Error = io::Error;

    fn try_from(item: IMMDevice) -> Result<Self, Self::Error> {
        let raw_id = unsafe { item.GetId()? };
        let _raw_id_memory = ComTaskMemory(raw_id.as_ptr());
        let property_store = unsafe { item.OpenPropertyStore(STGM_READ) }?;
        let friendly_name_prop: PROPVARIANT =
            unsafe { property_store.GetValue(&PKEY_Device_FriendlyName)? };
        let friendly_name = OsString::from_wide(unsafe {
            friendly_name_prop
                .Anonymous
                .Anonymous
                .Anonymous
                .pwszVal
                .as_wide()
        })
        .to_string_lossy()
        .to_string();
        let copy = AudioOutputDevice {
            id: OsString::from_wide(unsafe { raw_id.as_wide() }),
            friendly_name,
        };
        Ok(copy)
    }
}

impl PartialEq for AudioOutputDevice {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

mod policy_config {
    #![allow(non_upper_case_globals, non_snake_case)]

    use std::ffi::c_void;

    use windows::core::{
        ComInterface,
        Interface,
        GUID,
        PCWSTR,
    };
    use windows::Win32::Media::Audio::ERole;

    use crate::com::ComInterfaceExt;

    #[repr(transparent)]
    pub struct IPolicyConfig(windows::core::IUnknown);

    impl IPolicyConfig {
        pub unsafe fn SetDefaultEndpoint<P0, P1>(
            &self,
            deviceId: P0,
            eRole: P1,
        ) -> windows::core::Result<()>
        where
            P0: Into<PCWSTR>,
            P1: Into<ERole>,
        {
            (Interface::vtable(self).SetDefaultEndpoint)(
                Interface::as_raw(self),
                deviceId.into(),
                eRole.into(),
            )
            .ok()
        }
    }

    windows::imp::interface_hierarchy!(IPolicyConfig, windows::core::IUnknown);

    impl Clone for IPolicyConfig {
        fn clone(&self) -> Self {
            Self(self.0.clone())
        }
    }
    impl PartialEq for IPolicyConfig {
        fn eq(&self, other: &Self) -> bool {
            self.0 == other.0
        }
    }
    impl Eq for IPolicyConfig {}
    impl core::fmt::Debug for IPolicyConfig {
        fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
            f.debug_tuple("IPolicyConfig").field(&self.0).finish()
        }
    }

    unsafe impl Interface for IPolicyConfig {
        type Vtable = IPolicyConfig_Vtbl;
    }
    unsafe impl ComInterface for IPolicyConfig {
        const IID: GUID = GUID::from_u128(0xf8679f50_850a_41cf_9c72_430f290290c8);
    }

    #[repr(C)]
    #[allow(non_camel_case_types)]
    pub struct IPolicyConfig_Vtbl {
        pub base__: windows::core::IUnknown_Vtbl,
        padding: [*const c_void; 10], // Other fns may be added later
        pub SetDefaultEndpoint: unsafe extern "system" fn(
            this: *mut c_void,
            wszDeviceId: PCWSTR,
            eRole: ERole,
        ) -> windows::core::HRESULT,
        padding2: [*const c_void; 1], // Other fns may be added later
    }

    const CPolicyConfigClient: GUID = GUID::from_u128(0x870af99c_171d_4f9e_af0d_e63df40c2bc9);

    impl ComInterfaceExt for IPolicyConfig {
        const CLASS_GUID: GUID = CPolicyConfigClient;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn check_audio_device_list() -> io::Result<()> {
        let devices = AudioOutputDevice::get_active_devices()?;
        if let Some(device) = devices.get(0) {
            assert!(!device.id.is_empty());
        }
        Ok(())
    }

    #[test]
    fn check_get_global_default() {
        // Accept errors here since there may be no default
        if let Ok(device) = AudioOutputDevice::get_global_default() {
            std::hint::black_box(&device);
        }
    }
}
