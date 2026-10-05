//! Fonts shipped with the overlay, independent of the machine's installed fonts.
use std::borrow::Cow;

pub(crate) const DEFAULT: &str = "Inter";
pub(crate) const FAMILIES: &[&str] = &[
    DEFAULT,
    "Google Sans",
    "Nunito",
    "Orbitron",
    "Archivo Black",
];

pub(super) fn embedded(family: &str) -> Option<Cow<'static, [u8]>> {
    let data: &'static [u8] = match family {
        "Inter" => include_bytes!("../../assets/fonts/overlay/Inter.ttf"),
        "Google Sans" => include_bytes!("../../assets/fonts/overlay/GoogleSans.ttf"),
        "Nunito" => include_bytes!("../../assets/fonts/overlay/Nunito.ttf"),
        "Orbitron" => include_bytes!("../../assets/fonts/overlay/Orbitron.ttf"),
        "Archivo Black" => include_bytes!("../../assets/fonts/overlay/ArchivoBlack.ttf"),
        _ => return None,
    };
    Some(Cow::Borrowed(data))
}

// Include the copyright notices and licenses in the distributed settings document.
pub(crate) const LICENSES: &str = concat!(
    include_str!("../../assets/fonts/overlay/inter-OFL.txt"),
    "\n\n",
    include_str!("../../assets/fonts/overlay/googlesans-OFL.txt"),
    "\n\n",
    include_str!("../../assets/fonts/overlay/nunito-OFL.txt"),
    "\n\n",
    include_str!("../../assets/fonts/overlay/orbitron-OFL.txt"),
    "\n\n",
    include_str!("../../assets/fonts/overlay/archivoblack-OFL.txt"),
);
