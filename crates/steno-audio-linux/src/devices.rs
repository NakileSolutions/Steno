//! Périphériques audio : type de transport d'après les propriétés PulseAudio / PipeWire, tri
//! stable (défaut d'abord, puis nom en ordre naturel, puis identifiant), et signal sonore.
//!
//! L'identifiant (`id`) est le nom du nœud PulseAudio (`alsa_input.usb-…`), stable d'un
//! branchement à l'autre, comme l'UID Core Audio sur macOS.

use crate::protocol::AudioDeviceInfo;
use crate::text::natural_cmp;

/// Propriétés utiles d'une source ou d'un puits.
#[derive(Debug, Clone, Default)]
pub struct NodeProps {
    pub name: String,
    pub bus: Option<String>,
    pub api: Option<String>,
    pub form_factor: Option<String>,
    pub class: Option<String>,
    pub driver: Option<String>,
    pub profile: Option<String>,
}

/// `builtin | usb | bluetooth | virtual | aggregate | other`.
pub fn transport(p: &NodeProps) -> &'static str {
    let lower = |o: &Option<String>| o.as_deref().unwrap_or("").to_lowercase();
    let (bus, api, ff, class, driver, profile, name) = (lower(&p.bus), lower(&p.api), lower(&p.form_factor), lower(&p.class), lower(&p.driver), lower(&p.profile), p.name.to_lowercase());
    if driver.contains("combine") || name.contains("combined") {
        return "aggregate";
    }
    if bus == "bluetooth" || api.starts_with("bluez") || name.starts_with("bluez_") {
        return "bluetooth";
    }
    if bus == "usb" || name.contains(".usb-") {
        return "usb";
    }
    if class == "filter" || class == "abstract" || driver.contains("null") || driver.contains("virtual") || driver.contains("echo-cancel") || name.contains("echo-cancel") || name.contains("null") {
        return "virtual";
    }
    if name.contains("hdmi") || profile.contains("hdmi") || name.contains("displayport") {
        return "other";
    }
    if ff == "internal" || bus == "pci" || bus == "platform" || bus == "isa" || name.starts_with("alsa_") {
        return "builtin";
    }
    "other"
}

/// Casque ou écouteurs : port actif « headphones » / « headset » (prise jack), type d'appareil
/// (`device.form_factor`), ou sortie Bluetooth (presque toujours des écouteurs en réunion).
pub fn is_headphones(p: &NodeProps, active_port: Option<&str>) -> bool {
    let port = active_port.unwrap_or("").to_lowercase();
    let ff = p.form_factor.as_deref().unwrap_or("").to_lowercase();
    port.contains("headphone") || port.contains("headset") || matches!(ff.as_str(), "headphone" | "headset" | "hands-free" | "handset") || transport(p) == "bluetooth"
}

/// Tri stable et déterministe : défaut d'abord, puis nom (ordre naturel), puis identifiant.
pub fn sorted(mut devices: Vec<AudioDeviceInfo>) -> Vec<AudioDeviceInfo> {
    devices.sort_by(|a, b| b.is_default.cmp(&a.is_default).then_with(|| natural_cmp(&a.name, &b.name)).then_with(|| a.id.cmp(&b.id)));
    devices
}

/// Exclut les entrées sans identifiant ni nom et les doublons d'identifiant.
pub fn cleaned(devices: Vec<AudioDeviceInfo>) -> Vec<AudioDeviceInfo> {
    let mut out: Vec<AudioDeviceInfo> = vec![];
    for d in devices {
        if d.id.trim().is_empty() || d.name.trim().is_empty() || out.iter().any(|o| o.id == d.id) {
            continue;
        }
        out.push(d);
    }
    sorted(out)
}

// MARK: - Signal sonore

pub const TONE_RATE: u32 = 48_000;

/// Deux notes douces (mi5 puis la5, 55 + 65 ms ≈ 120 ms, crête ≈ −15 dBFS, fondus de 8 ms).
pub fn detect_tone() -> Vec<f32> {
    let notes = [(659.25f64, 0.055f64), (880.0, 0.065)];
    let gain = 0.18f64;
    let rate = TONE_RATE as f64;
    let mut out = vec![];
    for (freq, dur) in notes {
        let n = (dur * rate).round() as usize;
        let fade_n = ((0.008 * rate) as usize).min(n / 2).max(1);
        let w = 2.0 * std::f64::consts::PI * freq / rate;
        for i in 0..n {
            let mut env = 1.0;
            if i < fade_n {
                env = i as f64 / fade_n as f64;
            } else if i >= n - fade_n {
                env = (n - i) as f64 / fade_n as f64;
            }
            let decay = (-6.0 * i as f64 / rate).exp();
            out.push((gain * env * decay * (w * i as f64).sin()) as f32);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dev(id: &str, name: &str, d: bool) -> AudioDeviceInfo {
        AudioDeviceInfo { id: id.into(), name: name.into(), is_default: d, transport: "usb".into() }
    }

    #[test]
    fn transports() {
        let p = |name: &str, bus: Option<&str>, api: Option<&str>| NodeProps { name: name.into(), bus: bus.map(String::from), api: api.map(String::from), ..Default::default() };
        assert_eq!(transport(&p("alsa_input.pci-0000_00_1f.3.analog-stereo", Some("pci"), Some("alsa"))), "builtin");
        assert_eq!(transport(&p("alsa_input.usb-Jabra_Evolve-00.mono-fallback", Some("usb"), None)), "usb");
        assert_eq!(transport(&p("bluez_input.AA_BB", None, Some("bluez5"))), "bluetooth");
        assert_eq!(transport(&p("alsa_output.pci-0000_01_00.1.hdmi-stereo", Some("pci"), None)), "other");
        assert_eq!(transport(&NodeProps { name: "steno_null".into(), driver: Some("module-null-sink.c".into()), ..Default::default() }), "virtual");
        assert_eq!(transport(&NodeProps { name: "x".into(), driver: Some("module-combine-sink.c".into()), ..Default::default() }), "aggregate");
    }

    #[test]
    fn casques() {
        let internal = NodeProps { name: "alsa_output.pci-0000_00_1f.3.analog-stereo".into(), bus: Some("pci".into()), ..Default::default() };
        assert!(!is_headphones(&internal, Some("analog-output-speaker")));
        assert!(is_headphones(&internal, Some("analog-output-headphones")));
        assert!(is_headphones(&NodeProps { name: "bluez_output.AA_BB.1".into(), api: Some("bluez5".into()), ..Default::default() }, None));
        assert!(is_headphones(&NodeProps { name: "alsa_output.usb-Jabra".into(), bus: Some("usb".into()), form_factor: Some("headset".into()), ..Default::default() }, None));
        assert!(!is_headphones(&NodeProps { name: "alsa_output.usb-Speaker".into(), bus: Some("usb".into()), form_factor: Some("speaker".into()), ..Default::default() }, None));
    }

    #[test]
    fn tri_et_nettoyage() {
        let v = cleaned(vec![dev("b", "Micro 10", false), dev("a", "micro 2", false), dev("c", "Zeta", true), dev("a", "doublon", false), dev("", "vide", false)]);
        let ids: Vec<&str> = v.iter().map(|d| d.id.as_str()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]);
    }

    #[test]
    fn signal() {
        let t = detect_tone();
        assert_eq!(t.len(), (0.055f64 * 48_000.0).round() as usize + (0.065f64 * 48_000.0).round() as usize);
        let peak = t.iter().fold(0f32, |m, s| m.max(s.abs()));
        assert!(peak > 0.1 && peak < 0.19);
        assert_eq!(t[0], 0.0);
    }
}
