use dioxus::prelude::*;

const APP_IMAGE: Asset = asset!("/assets/app.png");
const ICON_GITHUB: Asset = asset!("/assets/icons/github.svg");
const ICON_BUG: Asset = asset!("/assets/icons/bug-bold.svg");
const ICON_USER: Asset = asset!("/assets/icons/user-bold.svg");
const ICON_CHANGELOG: Asset = asset!("/assets/icons/bill-list-bold.svg");
const ICON_INFO: Asset = asset!("/assets/icons/info-circle-linear.svg");

const ICON_UPDATE: Asset = asset!("/assets/icons/refresh-linear.svg");
const ICON_DOWNLOAD: Asset = asset!("/assets/icons/download-minimalistic-bold.svg");

const GITHUB_URL: &str = "https://github.com/vertopolkaLF/silence";
const RELEASES_URL: &str = "https://github.com/vertopolkaLF/silence/releases";
const ISSUES_URL: &str = "https://github.com/vertopolkaLF/silence/issues";

#[derive(Clone, Copy, PartialEq)]
enum MockUpdateState {
    Initial,
    Checking,
    Found,
    Downloading(f32),
}

pub fn render() -> Element {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let mut update_state = use_signal(|| MockUpdateState::Initial);

    let status_text = match *update_state.read() {
        MockUpdateState::Initial | MockUpdateState::Checking => "No updates found. Last checked: Just now",
        MockUpdateState::Found => "New version available!",
        MockUpdateState::Downloading(_) => "Downloading update...",
    };

    let status_class = match *update_state.read() {
        MockUpdateState::Found | MockUpdateState::Downloading(_) => "about-update-status highlight",
        _ => "about-update-status",
    };

    let progress_val = if let MockUpdateState::Downloading(p) = *update_state.read() { p } else { 0.0 };
    let progress_percent = progress_val as u32;

    rsx! {
        section { class: "about-panel",
            section {
                class: "about-hero",
                id: "about-overview",
                "data-settings-section": "true",
                div { class: "about-hero-visual",
                    div { class: "about-app-icon-frame",
                        img {
                            class: "about-app-icon",
                            src: APP_IMAGE,
                            alt: "silence! app icon"
                        }
                    }
                }
                div { class: "about-hero-copy",
                    div { class: "about-title-row",
                        h1 { "silence!" }
                        span { class: "about-version-pill", "{version}" }
                    }
                    p { class: "about-description", "A simple microphone mute/unmute utility with global hotkey support." }
                }
            }

            section { class: "about-card about-actions-card",
                div { class: "about-card-head",
                    span { class: "solar-icon about-card-icon", style: "--icon: url('{ICON_UPDATE}')" }
                    h2 { "Updates" }
                }
                div { class: "{status_class}", "{status_text}" }
                div { class: "about-update-wrapper",
                    // Initial / Checking Layer
                    div {
                        class: match *update_state.read() {
                            MockUpdateState::Initial | MockUpdateState::Checking => "about-update-layer active",
                            _ => "about-update-layer exit-up",
                        },
                        button {
                            class: "about-update-btn",
                            onclick: move |_| {
                                if *update_state.read() == MockUpdateState::Initial {
                                    update_state.set(MockUpdateState::Checking);
                                    spawn(async move {
                                        tokio::time::sleep(std::time::Duration::from_millis(1500)).await;
                                        update_state.set(MockUpdateState::Found);
                                    });
                                }
                            },
                            span {
                                class: if *update_state.read() == MockUpdateState::Checking { "solar-icon spinning" } else { "solar-icon" },
                                style: "--icon: url('{ICON_UPDATE}')"
                            }
                            div { class: "btn-text-switcher",
                                span {
                                    class: if *update_state.read() == MockUpdateState::Checking { "text-out" } else { "text-in" },
                                    "Check Update"
                                }
                                span {
                                    class: if *update_state.read() == MockUpdateState::Checking { "text-in" } else { "text-out check-in-text" },
                                    "Checking updates..."
                                }
                            }
                        }
                    }
                    
                    // Found Layer
                    div {
                        class: match *update_state.read() {
                            MockUpdateState::Found => "about-update-layer active",
                            MockUpdateState::Downloading(_) => "about-update-layer exit-up",
                            _ => "about-update-layer exit-down",
                        },
                        button {
                            class: "about-update-btn",
                            onclick: move |_| {
                                update_state.set(MockUpdateState::Downloading(0.0));
                                spawn(async move {
                                    for i in 1..=100 {
                                        tokio::time::sleep(std::time::Duration::from_millis(30)).await;
                                        update_state.set(MockUpdateState::Downloading(i as f32));
                                    }
                                    tokio::time::sleep(std::time::Duration::from_millis(1000)).await;
                                    update_state.set(MockUpdateState::Initial);
                                });
                            },
                            span { class: "solar-icon", style: "--icon: url('{ICON_DOWNLOAD}')" }
                            span { "Update silence!" }
                        }
                        button {
                            class: "about-update-btn",
                            onclick: move |_| {
                                let _ = crate::open_external(RELEASES_URL);
                            },
                            span { class: "solar-icon", style: "--icon: url('{ICON_CHANGELOG}')" }
                            span { "View Release" }
                        }
                    }

                    // Downloading Layer
                    div {
                        class: match *update_state.read() {
                            MockUpdateState::Downloading(_) => "about-update-layer active",
                            _ => "about-update-layer exit-down",
                        },
                        div { class: "about-update-progress",
                            span {
                                class: "about-update-progress-fill",
                                style: "--progress: {progress_val}%;"
                            }
                            span { class: "about-update-progress-copy",
                                span { class: "about-update-progress-label", "Downloading update..." }
                                span { class: "about-update-progress-value", "{progress_percent}%" }
                            }
                        }
                    }
                }
                button {
                    class: "about-text-button",
                    onclick: move |_| {
                        crate::send_test_push_notification();
                    },
                    "Send Push"
                }
            }

            section { class: "about-card about-actions-card",
                div { class: "about-card-head",
                    span { class: "solar-icon about-card-icon", style: "--icon: url('{ICON_INFO}')" }
                    h2 { "Resources" }
                }
                div { class: "about-action-grid",
                    button {
                        class: "about-action-btn",
                        onclick: move |_| {
                            let _ = crate::open_external(GITHUB_URL);
                        },
                        span { class: "solar-icon", style: "--icon: url('{ICON_GITHUB}')" }
                        span { "GitHub" }
                        span { class: "about-action-hint", "View source code" }
                    }
                    button {
                        class: "about-action-btn",
                        onclick: move |_| {
                            let _ = crate::open_external(RELEASES_URL);
                        },
                        span { class: "solar-icon", style: "--icon: url('{ICON_CHANGELOG}')" }
                        span { "Changelog" }
                        span { class: "about-action-hint", "See what's new" }
                    }
                    button {
                        class: "about-action-btn",
                        onclick: move |_| {
                            let _ = crate::open_external(ISSUES_URL);
                        },
                        span { class: "solar-icon", style: "--icon: url('{ICON_BUG}')" }
                        span { "Report Issue" }
                        span { class: "about-action-hint", "Found a bug?" }
                    }
                    button {
                        class: "about-action-btn",
                        onclick: move |_| {
                            let _ = crate::open_external("https://github.com/vertopolkaLF");
                        },
                        span { class: "solar-icon", style: "--icon: url('{ICON_USER}')" }
                        span { "Author" }
                        span { class: "about-action-hint", "@vertopolkaLF" }
                    }
                }
            }

        }
    }
}
