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

#[derive(Clone, Debug, PartialEq)]
enum UpdateUiState {
    Idle,
    Checking,
    UpToDate,
    Available(crate::updater::UpdateInfo),
    Installing,
    Failed(String),
}

pub fn render() -> Element {
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let mut update_state = use_signal(|| UpdateUiState::Idle);

    let status_text = match update_state.read().clone() {
        UpdateUiState::Idle => "Ready to check for updates.".to_string(),
        UpdateUiState::Checking => "Checking updates...".to_string(),
        UpdateUiState::UpToDate => "No updates found.".to_string(),
        UpdateUiState::Available(update) => format!("{} is available.", update.version),
        UpdateUiState::Installing => "Launching installer...".to_string(),
        UpdateUiState::Failed(message) => format!("Update failed: {message}"),
    };

    let status_class = match *update_state.read() {
        UpdateUiState::Available(_) | UpdateUiState::Installing => "about-update-status highlight",
        UpdateUiState::Failed(_) => "about-update-status error",
        _ => "about-update-status",
    };

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
                    div {
                        class: match *update_state.read() {
                            UpdateUiState::Idle | UpdateUiState::Checking | UpdateUiState::UpToDate | UpdateUiState::Failed(_) => "about-update-layer active",
                            _ => "about-update-layer exit-up",
                        },
                        button {
                            class: "about-update-btn",
                            disabled: matches!(*update_state.read(), UpdateUiState::Checking),
                            onclick: move |_| {
                                if matches!(*update_state.read(), UpdateUiState::Checking) {
                                    return;
                                }
                                update_state.set(UpdateUiState::Checking);
                                spawn(async move {
                                    match crate::check_for_update().await {
                                        Ok(crate::updater::UpdateCheck::Available(update)) => {
                                            update_state.set(UpdateUiState::Available(update));
                                        }
                                        Ok(crate::updater::UpdateCheck::UpToDate) => {
                                            update_state.set(UpdateUiState::UpToDate);
                                        }
                                        Err(err) => {
                                            update_state.set(UpdateUiState::Failed(err.to_string()));
                                        }
                                    }
                                });
                            },
                            span {
                                class: if matches!(*update_state.read(), UpdateUiState::Checking) { "solar-icon spinning" } else { "solar-icon" },
                                style: "--icon: url('{ICON_UPDATE}')"
                            }
                            div { class: "btn-text-switcher",
                                span {
                                    class: if matches!(*update_state.read(), UpdateUiState::Checking) { "text-out" } else { "text-in" },
                                    "Check Update"
                                }
                                span {
                                    class: if matches!(*update_state.read(), UpdateUiState::Checking) { "text-in" } else { "text-out check-in-text" },
                                    "Checking updates..."
                                }
                            }
                        }
                    }

                    div {
                        class: match *update_state.read() {
                            UpdateUiState::Available(_) => "about-update-layer active",
                            UpdateUiState::Installing => "about-update-layer exit-up",
                            _ => "about-update-layer exit-down",
                        },
                        button {
                            class: "about-update-btn",
                            onclick: move |_| {
                                let UpdateUiState::Available(update) = update_state.read().clone() else {
                                    return;
                                };
                                update_state.set(UpdateUiState::Installing);
                                if let Err(err) = crate::request_update_install(update) {
                                    update_state.set(UpdateUiState::Failed(err.to_string()));
                                }
                            },
                            span { class: "solar-icon", style: "--icon: url('{ICON_DOWNLOAD}')" }
                            span { "Update silence!" }
                        }
                        button {
                            class: "about-update-btn",
                            onclick: move |_| {
                                let target = match update_state.read().clone() {
                                    UpdateUiState::Available(update) => update.release_url,
                                    _ => RELEASES_URL.to_string(),
                                };
                                let _ = crate::open_external(&target);
                            },
                            span { class: "solar-icon", style: "--icon: url('{ICON_CHANGELOG}')" }
                            span { "View Release" }
                        }
                    }

                    div {
                        class: match *update_state.read() {
                            UpdateUiState::Installing => "about-update-layer active",
                            _ => "about-update-layer exit-down",
                        },
                        div { class: "about-update-progress",
                            span {
                                class: "about-update-progress-fill",
                                style: "--progress: 100%;"
                            }
                            span { class: "about-update-progress-copy",
                                span {
                                    class: "about-update-progress-label",
                                    "Launching installer..."
                                }
                                span { class: "about-update-progress-value", "" }
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
