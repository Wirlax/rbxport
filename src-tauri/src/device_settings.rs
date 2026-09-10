//! The device tabs: reading a stick's settings and writing them back.
//!
//! Wire shape and commands together, kept out of `commands.rs` because they
//! share nothing with the library. What is on the stick and what each field
//! means is `rbl_devices::settings`' business; this only translates.

use std::path::Path;

use rbl_devices::settings::{
    DevSetting, DeviceSettings, KeyDisplay, OverviewWaveform, WaveformColor, WaveformPosition,
};
use rbl_onelibrary::settings::{ColorName, MenuSlot, StickSettings};
use serde::{Deserialize, Serialize};

use crate::error::{AppError, AppResult, ErrorKind};

/// One browse category or sort option.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MenuSlotDto {
    pub id: i64,
    pub menu_item: i64,
    pub name: String,
    pub seq: i64,
    pub visible: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorNameDto {
    pub id: i64,
    pub name: String,
}

/// Everything the six tabs show. A few kilobytes at most: 22 categories, 17
/// sorts, 8 colours.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
// Four presence flags for four optional files; a wire struct, not a state machine.
#[allow(clippy::struct_excessive_bools)]
pub struct DeviceSettingsDto {
    /// Whether `export.pdb` is on the stick — "Device Library".
    pub has_device_library: bool,
    /// Whether `exportLibrary.db` is on the stick — "`OneLibrary`".
    pub has_one_library: bool,
    /// Whether `DEVSETTING.DAT` was read. When false the four display
    /// settings below are rekordbox's defaults and a write creates the file.
    pub has_dev_setting: bool,
    /// `blue` | `rgb` | `3band`
    pub waveform_color: String,
    /// `center` | `left`
    pub waveform_position: String,
    /// `half` | `full`
    pub overview_waveform: String,
    /// `classic` | `alphanumeric`
    pub key_display: String,
    /// Whether the library settings below were read; when false the stick
    /// has no `exportLibrary.db` and they are not written.
    pub has_library_settings: bool,
    pub device_name: String,
    pub background_color_type: i64,
    pub categories: Vec<MenuSlotDto>,
    pub sorts: Vec<MenuSlotDto>,
    pub sub_column: Option<i64>,
    pub colors: Vec<ColorNameDto>,
}

fn slot_dto(slot: &MenuSlot) -> MenuSlotDto {
    MenuSlotDto {
        id: slot.id,
        menu_item: slot.menu_item,
        name: slot.name.clone(),
        seq: slot.seq,
        visible: slot.visible,
    }
}

fn slot_from(dto: &MenuSlotDto) -> MenuSlot {
    MenuSlot {
        id: dto.id,
        menu_item: dto.menu_item,
        name: dto.name.clone(),
        seq: dto.seq,
        visible: dto.visible,
    }
}

pub fn to_dto(settings: &DeviceSettings) -> DeviceSettingsDto {
    let default_dev = DevSetting::default();
    let dev = settings.dev.as_ref().unwrap_or(&default_dev);
    let default_library = StickSettings::default();
    let library = settings.library.as_ref().unwrap_or(&default_library);
    DeviceSettingsDto {
        has_device_library: settings.has_device_library,
        has_one_library: settings.has_one_library,
        has_dev_setting: settings.dev.is_some(),
        waveform_color: match dev.color {
            WaveformColor::Blue => "blue",
            WaveformColor::Rgb => "rgb",
            WaveformColor::TriBand => "3band",
        }
        .to_owned(),
        waveform_position: match dev.position {
            WaveformPosition::Center => "center",
            WaveformPosition::Left => "left",
        }
        .to_owned(),
        overview_waveform: match dev.overview {
            OverviewWaveform::Half => "half",
            OverviewWaveform::Full => "full",
        }
        .to_owned(),
        key_display: match dev.key_display {
            KeyDisplay::Classic => "classic",
            KeyDisplay::Alphanumeric => "alphanumeric",
        }
        .to_owned(),
        has_library_settings: settings.library.is_some(),
        device_name: library.device_name.clone(),
        background_color_type: library.background_color_type,
        categories: library.categories.iter().map(slot_dto).collect(),
        sorts: library.sorts.iter().map(slot_dto).collect(),
        sub_column: library.sub_column,
        colors: library
            .colors
            .iter()
            .map(|c| ColorNameDto { id: c.id, name: c.name.clone() })
            .collect(),
    }
}

/// What to write, given what the stick currently holds.
///
/// The unexplained bytes of `DEVSETTING.DAT` come from the file as read, so
/// the current settings are the base and the wire values are applied over
/// them. A stick without the file gets the defaults plus the changes.
pub fn apply(current: &DeviceSettings, dto: &DeviceSettingsDto) -> AppResult<DeviceSettings> {
    let mut dev = current.dev.clone().unwrap_or_default();
    dev.color = match dto.waveform_color.as_str() {
        "blue" => WaveformColor::Blue,
        "rgb" => WaveformColor::Rgb,
        "3band" => WaveformColor::TriBand,
        other => return Err(bad_value("Waveform color", other)),
    };
    dev.position = match dto.waveform_position.as_str() {
        "center" => WaveformPosition::Center,
        "left" => WaveformPosition::Left,
        other => return Err(bad_value("Waveform Current Position", other)),
    };
    dev.overview = match dto.overview_waveform.as_str() {
        "half" => OverviewWaveform::Half,
        "full" => OverviewWaveform::Full,
        other => return Err(bad_value("Type of the Overview Waveform", other)),
    };
    dev.key_display = match dto.key_display.as_str() {
        "classic" => KeyDisplay::Classic,
        "alphanumeric" => KeyDisplay::Alphanumeric,
        other => return Err(bad_value("Key display format", other)),
    };

    // Library settings only go to a stick that has a library to hold them.
    let library = current.library.as_ref().map(|existing| StickSettings {
        device_name: dto.device_name.trim().to_owned(),
        // Never changed by the tabs: its values are not understood.
        background_color_type: existing.background_color_type,
        categories: dto.categories.iter().map(slot_from).collect(),
        sorts: dto.sorts.iter().map(slot_from).collect(),
        sub_column: dto.sub_column,
        colors: dto
            .colors
            .iter()
            .map(|c| ColorName { id: c.id, name: c.name.clone() })
            .collect(),
    });

    Ok(DeviceSettings {
        dev: Some(dev),
        library,
        has_device_library: current.has_device_library,
        has_one_library: current.has_one_library,
    })
}

fn bad_value(field: &str, value: &str) -> AppError {
    AppError::new(ErrorKind::Malformed, format!("{field}: {value:?} is not a choice."))
}

/// Reads a stick's settings. Never fails on a stick that holds nothing:
/// every part is optional and the tabs say what is missing.
#[tauri::command]
pub async fn device_settings(path: String) -> AppResult<DeviceSettingsDto> {
    crate::commands::blocking("device_settings", move || {
        Ok(to_dto(&rbl_devices::settings::read(Path::new(&path))))
    })
    .await
}

/// Writes a stick's settings back, and returns what the stick now holds.
#[tauri::command]
pub async fn save_device_settings(
    path: String,
    settings: DeviceSettingsDto,
) -> AppResult<DeviceSettingsDto> {
    crate::commands::blocking("save_device_settings", move || {
        let mount = Path::new(&path);
        if !mount.is_dir() {
            return Err(AppError::new(ErrorKind::NotFound, "That device is no longer connected."));
        }
        let current = rbl_devices::settings::read(mount);
        let next = apply(&current, &settings)?;
        rbl_devices::settings::write(mount, &next)
            .map_err(|e| AppError::new(ErrorKind::Internal, e.to_string()))?;
        Ok(to_dto(&rbl_devices::settings::read(mount)))
    })
    .await
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_stick_with_nothing_shows_defaults_and_writes_no_library() {
        let empty = DeviceSettings {
            dev: None,
            library: None,
            has_device_library: false,
            has_one_library: false,
        };
        let dto = to_dto(&empty);
        assert!(!dto.has_dev_setting);
        assert!(!dto.has_library_settings);
        assert_eq!(dto.waveform_color, "blue");
        assert_eq!(dto.waveform_position, "center");
        // The reference rows, so the tabs have something to draw, disabled.
        assert_eq!(dto.categories.len(), 22);

        let mut changed = dto.clone();
        changed.waveform_color = "rgb".to_owned();
        changed.device_name = "FRIDAY".to_owned();
        let next = apply(&empty, &changed).unwrap();
        assert_eq!(next.dev.unwrap().color, WaveformColor::Rgb);
        assert!(next.library.is_none(), "no exportLibrary.db to write a name into");
    }

    #[test]
    fn the_library_settings_round_trip_through_the_wire_shape() {
        let stick = DeviceSettings {
            dev: Some(DevSetting::default()),
            library: Some(StickSettings::default()),
            has_device_library: true,
            has_one_library: true,
        };
        let mut dto = to_dto(&stick);
        assert_eq!(dto.categories.len(), 22);
        assert_eq!(dto.sorts.len(), 17);
        assert_eq!(dto.colors.len(), 8);
        dto.colors[0].name = "Vocal".to_owned();
        dto.categories[0].visible = true;
        dto.categories[0].seq = 11;
        dto.sub_column = Some(2);
        dto.key_display = "alphanumeric".to_owned();

        let next = apply(&stick, &dto).unwrap();
        let library = next.library.unwrap();
        assert_eq!(library.colors[0].name, "Vocal");
        assert!(library.categories[0].visible);
        assert_eq!(library.sub_column, Some(2));
        assert_eq!(next.dev.unwrap().key_display, KeyDisplay::Alphanumeric);
    }

    #[test]
    fn a_value_that_is_not_a_choice_is_refused() {
        let stick = DeviceSettings {
            dev: None,
            library: None,
            has_device_library: false,
            has_one_library: false,
        };
        let mut dto = to_dto(&stick);
        dto.waveform_color = "plaid".to_owned();
        assert!(apply(&stick, &dto).is_err());
    }
}
