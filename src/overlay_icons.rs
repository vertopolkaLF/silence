use once_cell::sync::Lazy;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct OverlayIconPair {
    pub id: &'static str,
    pub label: &'static str,
    pub unmuted_asset: &'static str,
    pub muted_asset: &'static str,
    pub unmuted_svg: &'static str,
    pub muted_svg: &'static str,
}

macro_rules! pair {
    ($id:literal, $label:literal, $unmuted:literal, $muted:literal) => {
        OverlayIconPair {
            id: $id,
            label: $label,
            unmuted_asset: concat!("/assets/icons/overlay/", $unmuted),
            muted_asset: concat!("/assets/icons/overlay/", $muted),
            unmuted_svg: include_str!(concat!("../assets/icons/overlay/", $unmuted)),
            muted_svg: include_str!(concat!("../assets/icons/overlay/", $muted)),
        }
    };
}

const ICON_PAIRS: [OverlayIconPair; 18] = [
    pair!("fluent", "Fluent", "fluent-mic.svg", "fluent-mic-off.svg"),
    pair!(
        "solar",
        "Solar",
        "solar-microphone-3-linear.svg",
        "solar-microphone-3-broken.svg"
    ),
    pair!(
        "phosphor",
        "Phosphor",
        "ph-microphone.svg",
        "ph-microphone-slash.svg"
    ),
    pair!(
        "hugeicons",
        "Hugeicons",
        "hugeicons-mic-01.svg",
        "hugeicons-mic-off-01.svg"
    ),
    pair!("lucide", "Lucide", "lucide-mic.svg", "lucide-mic-off.svg"),
    pair!(
        "tabler",
        "Tabler",
        "tabler-microphone.svg",
        "tabler-microphone-off.svg"
    ),
    pair!(
        "tabler-fill",
        "Tabler Fill",
        "tabler-microphone-filled.svg",
        "tabler-microphone-off.svg"
    ),
    pair!(
        "material",
        "Material",
        "material-mic-outline.svg",
        "material-mic-off-outline.svg"
    ),
    pair!("mdi", "MDI", "mdi-microphone.svg", "mdi-microphone-off.svg"),
    pair!(
        "remix",
        "Remix",
        "remix-mic-line.svg",
        "remix-mic-off-line.svg"
    ),
    pair!(
        "iconamoon",
        "IconMoon",
        "iconamoon-microphone.svg",
        "iconamoon-microphone-off.svg"
    ),
    pair!(
        "gravity",
        "Gravity",
        "gravity-microphone.svg",
        "gravity-microphone-slash.svg"
    ),
    pair!(
        "eva",
        "Eva",
        "eva-mic-outline.svg",
        "eva-mic-off-outline.svg"
    ),
    pair!(
        "uicons",
        "UIcons",
        "uil-microphone.svg",
        "uil-microphone-slash.svg"
    ),
    pair!(
        "basil",
        "Basil",
        "basil-microphone-outline.svg",
        "basil-microphone-off-outline.svg"
    ),
    pair!(
        "pepicons",
        "Pepicons",
        "pepicons-microphone.svg",
        "pepicons-microphone-off.svg"
    ),
    pair!(
        "mingcute",
        "MingCute",
        "mingcute-mic.svg",
        "mingcute-mic-off.svg"
    ),
    pair!(
        "mingcute-fill",
        "Ming Fill",
        "mingcute-mic-fill.svg",
        "mingcute-mic-off-fill.svg"
    ),
];

static FEATURED_ICON_PAIRS: Lazy<Vec<OverlayIconPair>> = Lazy::new(|| {
    let featured_ids = ["fluent", "solar", "phosphor", "hugeicons", "lucide"];
    featured_ids
        .iter()
        .map(|id| *overlay_icon_pair(id))
        .collect()
});

static EXTRA_ICON_PAIRS: Lazy<Vec<OverlayIconPair>> = Lazy::new(|| {
    let featured_ids = ["fluent", "solar", "phosphor", "hugeicons", "lucide"];
    ICON_PAIRS
        .iter()
        .copied()
        .filter(|pair| !featured_ids.contains(&pair.id))
        .collect()
});

pub fn default_overlay_icon_pair() -> String {
    "fluent".to_string()
}

pub fn featured_overlay_icon_pairs() -> &'static [OverlayIconPair] {
    &FEATURED_ICON_PAIRS
}

pub fn extra_overlay_icon_pairs() -> &'static [OverlayIconPair] {
    &EXTRA_ICON_PAIRS
}

pub fn overlay_icon_pair(id: &str) -> &'static OverlayIconPair {
    ICON_PAIRS
        .iter()
        .find(|pair| pair.id == id)
        .unwrap_or(&ICON_PAIRS[0])
}

pub fn overlay_icon_svg(id: &str, muted: bool) -> &'static str {
    let pair = overlay_icon_pair(id);
    if muted {
        pair.muted_svg
    } else {
        pair.unmuted_svg
    }
}

pub fn overlay_icon_css_url(id: &str, muted: bool) -> String {
    let svg = overlay_icon_svg(id, muted);
    format!("data:image/svg+xml;utf8,{}", encode_svg(svg))
}

fn encode_svg(svg: &str) -> String {
    let mut encoded = String::with_capacity(svg.len() * 2);
    for byte in svg.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => {
                encoded.push('%');
                encoded.push_str(&format!("{byte:02X}"));
            }
        }
    }
    encoded
}
