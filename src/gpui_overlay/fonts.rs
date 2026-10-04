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

pub(super) fn embedded() -> Vec<Cow<'static, [u8]>> {
    vec![
        Cow::Borrowed(include_bytes!("../../assets/fonts/overlay/Inter.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/overlay/GoogleSans.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/overlay/Nunito.ttf")),
        Cow::Borrowed(include_bytes!("../../assets/fonts/overlay/Orbitron.ttf")),
        Cow::Borrowed(include_bytes!(
            "../../assets/fonts/overlay/ArchivoBlack.ttf"
        )),
    ]
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
