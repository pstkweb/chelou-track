// Inventaire des cartes son par pilote, pour l'écran de réglages, et recherche d'un périphérique
// choisi par son nom.

use std::str::FromStr;

use anyhow::{anyhow, Context, Result};
use cpal::traits::{DeviceTrait, HostTrait};
use cpal::{SupportedBufferSize, SupportedStreamConfigRange};
use serde::Serialize;

/// Pilotes proposés, dans l'ordre d'affichage (ASIO recommandé sur Windows).
const HOSTS: &[&str] = &["asio", "wasapi", "coreaudio", "alsa"];
/// Fréquences proposées à l'utilisateur.
const RATES: &[u32] = &[44100, 48000, 88200, 96000, 176400, 192000];

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub name: String,
    pub inputs: u16,
    pub outputs: u16,
    pub sample_rates: Vec<u32>,
    pub buffer_min: Option<u32>,
    pub buffer_max: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HostDevices {
    /// `"asio" | "wasapi" | "coreaudio" | "alsa"`.
    pub host: String,
    pub devices: Vec<DeviceInfo>,
}

/// Cartes de chaque pilote disponible. En ASIO, un seul pilote peut être chargé à la fois : tant
/// qu'une carte ASIO est ouverte, les autres n'apparaissent pas (cf. `Engine::devices`).
pub fn list_devices() -> Vec<HostDevices> {
    let mut ids: Vec<_> = cpal::available_hosts()
        .into_iter()
        .filter_map(|id| Some((HOSTS.iter().position(|h| *h == id.to_string())?, id)))
        .collect();
    ids.sort_by_key(|(order, _)| *order);
    ids.into_iter()
        .filter_map(|(_, id)| {
            let host = cpal::host_from_id(id).ok()?;
            let devices = host.devices().ok()?.filter_map(|d| describe(&d)).collect();
            Some(HostDevices {
                host: id.to_string(),
                devices,
            })
        })
        .collect()
}

pub(crate) fn device_name(device: &cpal::Device) -> Option<String> {
    device.description().ok().map(|d| d.name().to_string())
}

pub(crate) fn describe(device: &cpal::Device) -> Option<DeviceInfo> {
    let name = device_name(device)?;
    let configs = |list: Result<Vec<SupportedStreamConfigRange>, _>| list.unwrap_or_default();
    let inputs = configs(device.supported_input_configs().map(Iterator::collect));
    let outputs = configs(device.supported_output_configs().map(Iterator::collect));
    if inputs.is_empty() && outputs.is_empty() {
        return None;
    }
    let max_channels =
        |list: &[SupportedStreamConfigRange]| list.iter().map(|c| c.channels()).max().unwrap_or(0);
    let all = || inputs.iter().chain(&outputs);
    let sample_rates = RATES
        .iter()
        .copied()
        .filter(|&r| all().any(|c| c.min_sample_rate() <= r && r <= c.max_sample_rate()))
        .collect();
    let buffer = all().find_map(|c| match *c.buffer_size() {
        SupportedBufferSize::Range { min, max } => Some((min, max)),
        SupportedBufferSize::Unknown => None,
    });
    Some(DeviceInfo {
        name,
        inputs: max_channels(&inputs),
        outputs: max_channels(&outputs),
        sample_rates,
        buffer_min: buffer.map(|b| b.0),
        buffer_max: buffer.map(|b| b.1),
    })
}

pub(crate) fn find_host(name: &str) -> Result<cpal::Host> {
    let id = cpal::HostId::from_str(name).map_err(|_| anyhow!("Pilote {name} indisponible"))?;
    cpal::host_from_id(id).with_context(|| format!("ouverture du pilote {name}"))
}

pub(crate) fn find_device(host: &cpal::Host, name: &str, input: bool) -> Result<cpal::Device> {
    let devices: Vec<_> = if input {
        host.input_devices()?.collect()
    } else {
        host.output_devices()?.collect()
    };
    devices
        .into_iter()
        .find(|d| device_name(d).as_deref() == Some(name))
        .ok_or_else(|| anyhow!("Carte « {name} » introuvable"))
}
