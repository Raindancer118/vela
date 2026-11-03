//! Filesystems a stick can be formatted with and their label rules.

use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Fs {
    Fat32,
    Exfat,
    Ntfs,
    Ext4,
}

impl Fs {
    pub const ALL: [Fs; 4] = [Fs::Fat32, Fs::Exfat, Fs::Ntfs, Fs::Ext4];

    pub fn from_id(id: &str) -> Option<Fs> {
        Fs::ALL.into_iter().find(|f| f.id() == id)
    }

    pub fn id(self) -> &'static str {
        match self {
            Fs::Fat32 => "fat32",
            Fs::Exfat => "exfat",
            Fs::Ntfs => "ntfs",
            Fs::Ext4 => "ext4",
        }
    }

    /// Type udisks formats with.
    pub fn udisks_type(self) -> &'static str {
        match self {
            Fs::Fat32 => "vfat",
            Fs::Exfat => "exfat",
            Fs::Ntfs => "ntfs",
            Fs::Ext4 => "ext4",
        }
    }

    /// MBR partition type: what Windows and cameras look for.
    pub fn mbr_type(self) -> &'static str {
        match self {
            Fs::Fat32 => "0x0c",
            Fs::Exfat | Fs::Ntfs => "0x07",
            Fs::Ext4 => "0x83",
        }
    }

    /// Longest label: FAT 11 bytes, exFAT 15 and NTFS 32 UTF-16 units, ext4 16 bytes.
    fn max_label(self) -> usize {
        match self {
            Fs::Fat32 => 11,
            Fs::Exfat => 15,
            Fs::Ntfs => 32,
            Fs::Ext4 => 16,
        }
    }

    /// A label the filesystem takes: FAT wants upper-case ASCII without
    /// `"*+,./:;<=>?[\]|`; everything is cut to its length.
    pub fn label(self, wanted: &str) -> String {
        let wanted = wanted.trim();
        match self {
            Fs::Fat32 => wanted
                .chars()
                .map(|c| match c {
                    'ä' | 'Ä' => 'A',
                    'ö' | 'Ö' => 'O',
                    'ü' | 'Ü' => 'U',
                    'ß' => 'S',
                    c => c,
                })
                .filter(|c| c.is_ascii() && !c.is_ascii_control() && !"\"*+,./:;<=>?[\\]|".contains(*c))
                .map(|c| c.to_ascii_uppercase())
                .take(self.max_label())
                .collect::<String>()
                .trim_end()
                .to_owned(),
            Fs::Exfat | Fs::Ntfs => {
                let mut out = String::new();
                let mut units = 0;
                for c in wanted.chars().filter(|c| !c.is_control()) {
                    units += c.len_utf16();
                    if units > self.max_label() {
                        break;
                    }
                    out.push(c);
                }
                out
            }
            Fs::Ext4 => {
                let mut out = String::new();
                for c in wanted.chars().filter(|c| !c.is_control()) {
                    if out.len() + c.len_utf8() > self.max_label() {
                        break;
                    }
                    out.push(c);
                }
                out
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_fit_the_filesystem() {
        assert_eq!(Fs::Fat32.label("Mein Stick: 2026/11"), "MEIN STICK");
        assert_eq!(Fs::Fat32.label("Fotos Ärger"), "FOTOS ARGER");
        assert_eq!(Fs::Fat32.label("  usb  "), "USB");
        assert_eq!(Fs::Exfat.label("Ein sehr langer Name"), "Ein sehr langer");
        assert_eq!(Fs::Ext4.label("Daten für später"), "Daten für spät");
        assert_eq!(Fs::Ntfs.label("Übertragung"), "Übertragung");
        assert_eq!(Fs::from_id("exfat"), Some(Fs::Exfat));
        assert_eq!(Fs::from_id("btrfs"), None);
        assert_eq!((Fs::Fat32.udisks_type(), Fs::Fat32.mbr_type()), ("vfat", "0x0c"));
    }
}
