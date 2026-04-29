use dioxus::prelude::*;

const APP_IMAGE: Asset = asset!("/assets/app.png");
const GITHUB_URL: &str = "https://github.com/vertopolkaLF/silence";
const RELEASES_URL: &str = "https://github.com/vertopolkaLF/silence/releases";

pub fn render() -> Element {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));

    rsx! {
        section { class: "about-panel",
            section {
                class: "about-hero",
                id: "about-overview",
                "data-settings-section": "true",
                img {
                    class: "about-app-icon",
                    src: APP_IMAGE,
                    alt: "silence! app icon"
                }
                div { class: "about-hero-copy",
                    div { class: "about-title-row section-head-row",
                        h1 { "silence!" }
                        span { class: "about-version-pill", "{version}" }
                    }
                    p { "A simple microphone mute/unmute utility with global hotkey support." }
                }
            }

            section { class: "about-card",
                div { class: "about-card-head",
                    span { class: "solar-icon icon-info about-card-icon" }
                    h2 { "Updates" }
                }
                div { class: "about-update-status",
                    div { class: "about-update-copy",
                        h3 { "Current build" }
                        p { "You're using {version}." }
                    }
                    div { class: "about-actions",
                        button {
                            class: "secondary",
                            onclick: move |_| {
                                let _ = crate::open_external(RELEASES_URL);
                            },
                            "View Release"
                        }
                        button {
                            class: "secondary",
                            onclick: move |_| {
                                let _ = crate::open_external(GITHUB_URL);
                            },
                            "View on GitHub"
                        }
                    }
                }
            }

            section { class: "about-card about-signoff-card",
                p { class: "about-made-by", "Made with love by vertopolkaLF" }
            }

            section { class: "about-card about-credits-card",
                div { class: "about-card-head",
                    span { class: "solar-icon icon-shield about-card-icon" }
                    h2 { "Credits" }
                }
                ul { class: "about-credits-list",
                    li { "WinUI 3 - Microsoft" }
                    li { "H.NotifyIcon - havendv" }
                }
            }
        }
    }
}
