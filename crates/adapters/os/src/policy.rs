use mivia_ports::policy::{Hive, PolicyValue, RegistryValue};
use winreg::enums::{
    HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_BINARY, REG_DWORD, REG_DWORD_BIG_ENDIAN,
    REG_EXPAND_SZ, REG_FULL_RESOURCE_DESCRIPTOR, REG_LINK, REG_MULTI_SZ, REG_NONE, REG_QWORD,
    REG_RESOURCE_LIST, REG_RESOURCE_REQUIREMENTS_LIST, REG_SZ,
};
use winreg::types::FromRegValue;
use winreg::{HKEY, RegKey, RegValue};

use crate::ffi;

fn convert(raw: &RegValue) -> RegistryValue {
    let text = || String::from_reg_value(raw).ok();
    let value = match raw.vtype {
        REG_SZ => text().map(RegistryValue::Text),
        REG_EXPAND_SZ => {
            text().map(|text| RegistryValue::Expandable(ffi::expand_environment(&text)))
        }
        REG_DWORD => u32::from_reg_value(raw).ok().map(RegistryValue::Number),
        REG_NONE
        | REG_BINARY
        | REG_DWORD_BIG_ENDIAN
        | REG_LINK
        | REG_MULTI_SZ
        | REG_RESOURCE_LIST
        | REG_FULL_RESOURCE_DESCRIPTOR
        | REG_RESOURCE_REQUIREMENTS_LIST
        | REG_QWORD => None,
    };
    value.unwrap_or(RegistryValue::Other)
}

fn read(hive: Hive, predef: HKEY, key: &str) -> Vec<PolicyValue> {
    let Ok(opened) = RegKey::predef(predef).open_subkey_with_flags(key, KEY_READ) else {
        return Vec::new();
    };
    opened
        .enum_values()
        .filter_map(Result::ok)
        .map(|(name, raw)| PolicyValue {
            hive,
            name,
            value: convert(&raw),
        })
        .collect()
}

pub fn read_policies(key: &str) -> Vec<PolicyValue> {
    let mut values = read(Hive::Machine, HKEY_LOCAL_MACHINE, key);
    values.extend(read(Hive::User, HKEY_CURRENT_USER, key));
    values
}
